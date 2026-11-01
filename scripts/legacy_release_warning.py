#!/usr/bin/env python3
"""Prepend an idempotent Ubuntu pet notice without changing old release text."""

import argparse
import json
from pathlib import Path


MARKER = "<!-- quotamate-legacy-ubuntu-pet-warning:v1 -->"
WARNING = f"""{MARKER}
> **Ubuntu 桌面宠物提示 / Ubuntu desktop-pet notice**
>
> 此版本的 Ubuntu 宠物安装包存在已知的透明残影与窗口交互问题，请使用[最新版本](https://github.com/yueyisui/QuotaMate/releases/latest)。Windows/macOS 文件保留，原发布说明见下方。
>
> This version's Ubuntu pet packages have known transparency and window-interaction issues. Use the [latest release](https://github.com/yueyisui/QuotaMate/releases/latest). Windows/macOS files and the original release notes below are retained.

---

"""


def prepend_warning(body):
    return body if MARKER in body else WARNING + body


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("body_json", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    body = json.loads(args.body_json.read_text())["body"] or ""
    args.output.write_text(prepend_warning(body))


if __name__ == "__main__":
    main()
