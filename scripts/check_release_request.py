#!/usr/bin/env python3
"""Validate an explicit release request; print its pinned source SHA, or nothing.

Only a newly added .github/release-requests/vVERSION-SOURCE12.json in this main push
can authorize publication. The request commit must contain the frozen source
unchanged outside that directory. This file never reads credentials.
"""

import argparse
import json
import os
import re
import subprocess
from pathlib import Path


REQUEST_DIR = ".github/release-requests/"
VERSION = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+")
OBJECT_ID = re.compile(r"[0-9a-f]{40}")
SHA256 = re.compile(r"[0-9a-f]{64}")


class InvalidRequest(ValueError):
    pass


def git(repo, *args, check=True):
    result = subprocess.run(
        ["git", "-C", str(repo), *args], capture_output=True, text=True
    )
    if check and result.returncode:
        raise InvalidRequest(f"git {args[0]} failed: {result.stderr.strip()}")
    return result


def require(condition, message):
    if not condition:
        raise InvalidRequest(message)


def text_field(value):
    return isinstance(value, str) and bool(value.strip())


def read_file(repo, commit, path):
    return git(repo, "show", f"{commit}:{path}").stdout


def validate_versions(repo, source, version):
    package = json.loads(read_file(repo, source, "package.json"))
    tauri = json.loads(read_file(repo, source, "src-tauri/tauri.conf.json"))
    cargo = read_file(repo, source, "src-tauri/Cargo.toml")
    lock = read_file(repo, source, "src-tauri/Cargo.lock")
    cargo_version = re.search(r'(?m)^version = "([^"]+)"$', cargo)
    lock_versions = re.findall(
        r'(?m)^name = "quotamate"\nversion = "([^"]+)"$', lock
    )
    require(
        package.get("version") == tauri.get("version") == version
        and cargo_version is not None
        and cargo_version.group(1) == version
        and lock_versions == [version],
        "All four application versions must match the request",
    )
    require(
        read_file(repo, source, f"docs/releases/v{version}.md").strip(),
        "Release notes must not be empty",
    )


def validate_desktop(request, version):
    authorization = request.get("authorization")
    require(
        isinstance(authorization, dict)
        and authorization.get("publish_release") is True
        and text_field(authorization.get("requested_by"))
        and text_field(authorization.get("statement")),
        "An explicit publication authorization is required",
    )
    desktop = request.get("desktop_validation")
    require(
        isinstance(desktop, dict)
        and desktop.get("passed") is True
        and text_field(desktop.get("environment")),
        "Real desktop validation and its environment are required",
    )
    for kind, extension in (("deb", "deb"), ("appimage", "AppImage")):
        package = desktop.get(kind)
        require(isinstance(package, dict), f"Missing desktop validation: {kind}")
        require(
            package.get("file") == f"QuotaMate_{version}_amd64.{extension}"
            and isinstance(package.get("sha256"), str)
            and SHA256.fullmatch(package["sha256"])
            and type(package.get("passed")) is int
            and type(package.get("total")) is int
            and package["passed"] == package["total"] == 32
            and text_field(package.get("report")),
            f"{kind} requires the versioned filename, SHA-256 and all 32 desktop checks",
        )


def tag_exists(repo, tag):
    result = git(
        repo, "ls-remote", "--exit-code", "--tags", "origin", f"refs/tags/{tag}",
        check=False,
    )
    require(result.returncode in (0, 2), "Could not verify remote tag absence")
    return result.returncode == 0


def select_request(repo, event_name, ref, before, head):
    if event_name != "push" or ref != "refs/heads/main":
        return None
    # A new repository/unknown push base cannot prove a newly added request.
    if not isinstance(before, str) or not OBJECT_ID.fullmatch(before) or before == "0" * 40:
        return None
    changes = git(
        repo, "diff", "--name-status", "--no-renames", before, head, "--", REQUEST_DIR
    ).stdout.splitlines()
    added = [line.split("\t", 1)[1] for line in changes if line.startswith("A\t")]
    added = [path for path in added if path.endswith(".json")]
    if not added:
        return None
    require(len(added) == 1, "A push must add exactly one release request")
    require(all(line.startswith("A\t") for line in changes), "Existing request records must remain unchanged")
    path = added[0]
    request = json.loads(read_file(repo, head, path))
    require(isinstance(request, dict), "Release request must be an object")
    require(type(request.get("schema")) is int and request["schema"] == 1, "Unknown request schema")
    version = request.get("version")
    require(isinstance(version, str) and VERSION.fullmatch(version), "Invalid release version")
    require(request.get("publish") is True, "Request must explicitly enable publication")
    source = request.get("source_sha")
    tree = request.get("source_tree")
    require(isinstance(source, str) and OBJECT_ID.fullmatch(source), "Invalid source commit")
    require(isinstance(tree, str) and OBJECT_ID.fullmatch(tree), "Invalid source tree")
    require(
        path == f"{REQUEST_DIR}v{version}-{source[:12]}.json",
        "Request path must match its version and source commit",
    )
    require(source != head, "Request must reference an independent frozen source commit")
    require(
        git(repo, "merge-base", "--is-ancestor", source, head, check=False).returncode == 0,
        "Frozen source must be an ancestor of the request commit",
    )
    require(git(repo, "rev-parse", f"{source}^{{tree}}").stdout.strip() == tree, "Source tree mismatch")
    require(
        git(
            repo, "diff", "--quiet", source, head, "--", ".",
            f":(exclude){REQUEST_DIR}**", check=False,
        ).returncode == 0,
        "Request commit changed tracked source outside the request directory",
    )
    validate_versions(repo, source, version)
    validate_desktop(request, version)
    require(not tag_exists(repo, f"v{version}"), "Release request cannot reuse an existing tag")
    return source


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", type=Path, default=Path("."))
    parser.add_argument("--event-name", default=os.environ.get("GITHUB_EVENT_NAME", ""))
    parser.add_argument("--ref", default=os.environ.get("GITHUB_REF", ""))
    parser.add_argument("--head", default=os.environ.get("GITHUB_SHA", "HEAD"))
    parser.add_argument("--event-path", type=Path, default=os.environ.get("GITHUB_EVENT_PATH"))
    parser.add_argument("--expected-source", help="Require this selected source before publication")
    args = parser.parse_args()
    try:
        event = json.loads(args.event_path.read_text()) if args.event_path else {}
        selected = select_request(args.repo, args.event_name, args.ref, event.get("before"), args.head)
        if args.expected_source:
            require(selected == args.expected_source, "Publication source no longer matches the request")
        if selected:
            print(selected)
    except (InvalidRequest, ValueError, OSError, KeyError) as error:
        parser.exit(1, f"Release request rejected: {error}\n")


if __name__ == "__main__":
    main()
