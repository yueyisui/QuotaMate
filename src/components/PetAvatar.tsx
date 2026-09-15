import type { CSSProperties } from "react";
import type { CodexUsage } from "../types";
import { petName, translate, type LanguagePreference } from "../i18n";

export type BuiltInPet = "cat" | "dog" | "rocket" | "car" | "robot";

export const PET_PRESETS: { id: BuiltInPet; label: string }[] = [
  { id: "cat", label: "紫色小狐" },
  { id: "dog", label: "小狗" },
  { id: "rocket", label: "火箭" },
  { id: "car", label: "汽车" },
  { id: "robot", label: "机器人" },
];

export function usageEnergy(usage: CodexUsage) {
  const value = usage.fiveHour?.remainingPercent ?? usage.weekly?.remainingPercent;
  return typeof value === "number" ? Math.max(0, Math.min(100, value)) : 0;
}

export function energyState(energy: number, language?: LanguagePreference) {
  if (energy >= 75) return { key: "full", label: translate(language, "energyFull"), color: "#4f7cff" };
  if (energy >= 45) return { key: "good", label: translate(language, "energyGood"), color: "#26b987" };
  if (energy >= 20) return { key: "low", label: translate(language, "energyLow"), color: "#efa63a" };
  return { key: "empty", label: translate(language, "energyEmpty"), color: "#ed5b68" };
}

interface Props {
  preset: BuiltInPet;
  energy: number;
  className?: string;
  language?: LanguagePreference;
}

export function PetAvatar({ preset, energy, className = "", language }: Props) {
  const state = energyState(energy, language);
  const style = { "--pet-energy": state.color, "--pet-level": `${energy}%` } as CSSProperties;
  const common = {
    className: `pet-svg pet-svg--${preset} pet-svg--${state.key} ${className}`,
    style,
    viewBox: "0 0 200 200",
    role: "img" as const,
    "aria-label": `${petName(language, preset)}, ${state.label}`,
  };
  if (preset === "dog") return <Dog {...common} energy={energy} />;
  if (preset === "rocket") return <Rocket {...common} energy={energy} />;
  if (preset === "car") return <Car {...common} energy={energy} />;
  if (preset === "robot") return <Robot {...common} energy={energy} />;
  return <Cat {...common} energy={energy} />;
}

type SvgProps = React.SVGProps<SVGSVGElement> & { energy: number };

function Face({ energy, y = 88 }: { energy: number; y?: number }) {
  if (energy < 20) {
    return <><path d={`M68 ${y}l10 7m0-7-10 7M122 ${y}l10 7m0-7-10 7`} className="pet-ink" /><path d={`M88 ${y + 25}q12-10 24 0`} className="pet-ink" /></>;
  }
  if (energy < 45) {
    return <><circle cx="75" cy={y + 3} r="5" className="pet-eye" /><circle cx="125" cy={y + 3} r="5" className="pet-eye" /><path d={`M91 ${y + 23}h18`} className="pet-ink" /></>;
  }
  return <><path d={`M66 ${y + 3}q9-10 18 0M116 ${y + 3}q9-10 18 0`} className="pet-ink" /><path d={`M87 ${y + 19}q13 15 26 0`} className="pet-ink" /></>;
}

function Meter({ energy, x = 72, y = 139 }: { energy: number; x?: number; y?: number }) {
  return <g className="pet-meter"><rect x={x} y={y} width="56" height="18" rx="7" /><rect x={x + 4} y={y + 4} width={48 * energy / 100} height="10" rx="4" className="pet-meter__fill" /><path d={`M${x + 59} ${y + 6}v6`} /></g>;
}

function Cat({ energy, ...props }: SvgProps) {
  return <svg {...props}>
    <g className="fox-tail"><path d="M66 126q-45 5-38 39 7 28 42 9 20-12 21-31" className="pet-body" /><path d="M34 157q13 24 38 8" className="fox-tail-tip" /></g>
    <g className="fox-ear fox-ear--left"><path d="M48 77Q30 18 78 48" className="pet-body" /><path d="M51 65 45 39l24 18" className="pet-detail" /></g>
    <g className="fox-ear fox-ear--right"><path d="M152 77q18-59-30-29" className="pet-body" /><path d="m149 65 6-26-24 18" className="pet-detail" /></g>
    <ellipse cx="100" cy="101" rx="61" ry="58" className="pet-body fox-head" />
    <path d="M53 104q20 7 47 45 27-38 47-45-4 48-47 51-43-3-47-51" className="fox-muzzle" />
    <g className="fox-face"><Face energy={energy} y={81} /></g>
    <path d="m93 112q7-7 14 0-7 10-14 0" className="pet-nose-solid" />
    <circle cx="48" cy="95" r="4" className="fox-sparkle" /><circle cx="156" cy="83" r="3" className="fox-sparkle fox-sparkle--late" />
    <Meter energy={energy} x={72} y={157} />
  </svg>;
}

function Dog({ energy, ...props }: SvgProps) {
  return <svg {...props}>
    <circle cx="100" cy="101" r="78" className="pet-aura" />
    <path d="M48 58Q13 54 27 116q19 8 35-15M152 58q35-4 21 58-19 8-35-15" className="pet-ear" />
    <ellipse cx="100" cy="96" rx="58" ry="61" className="pet-body" />
    <path d="M65 56q17-27 35-8 18-19 35 8" className="pet-detail" />
    <Face energy={energy} y={78} />
    <ellipse cx="100" cy="108" rx="20" ry="16" className="pet-muzzle" />
    <path d="M94 103q6-5 12 0l-6 6z" className="pet-nose-solid" />
    <Meter energy={energy} />
  </svg>;
}

function Rocket({ energy, ...props }: SvgProps) {
  const flame = 12 + energy * .28;
  const fuelHeight = 62 * energy / 100;
  return <svg {...props}>
    <circle cx="100" cy="98" r="76" className="pet-aura" />
    <path d={`M82 151q18 ${flame} 36 0l-8-23H90z`} className="rocket-flame" />
    <path d="M72 128l-28 23 7-43 25-13m52 33 28 23-7-43-25-13" className="rocket-fin" />
    <path d="M72 124Q63 55 100 20q37 35 28 104l-14 22H86z" className="rocket-body" />
    <path d="M100 20q18 20 24 49H76q6-29 24-49z" className="rocket-cap" />
    <circle cx="100" cy="86" r="20" className="rocket-window" />
    <circle cx="100" cy="86" r="12" className="rocket-window-inner" />
    <rect x="94" y={132 - fuelHeight} width="12" height={fuelHeight} rx="6" className="rocket-fuel" />
  </svg>;
}

function Car({ energy, ...props }: SvgProps) {
  const bars = Math.ceil(energy / 25);
  return <svg {...props}>
    <circle cx="100" cy="104" r="76" className="pet-aura" />
    <path d="M43 103l19-40h75l24 40 14 10v37H25v-37z" className="car-body" />
    <path d="M69 72h26v30H54zm35 0h27l17 30h-44z" className="car-window" />
    <circle cx="58" cy="150" r="20" className="car-wheel" /><circle cx="145" cy="150" r="20" className="car-wheel" />
    <circle cx="58" cy="150" r="8" className="car-hub" /><circle cx="145" cy="150" r="8" className="car-hub" />
    <path d="M30 119h18m105 0h18" className="car-light" />
    <g className="car-battery">{[0,1,2,3].map((index) => <rect key={index} x={70 + index * 17} y="119" width="13" height="11" rx="3" className={index < bars ? "is-on" : ""} />)}</g>
  </svg>;
}

function Robot({ energy, ...props }: SvgProps) {
  return <svg {...props}>
    <circle cx="100" cy="101" r="78" className="pet-aura" />
    <path d="M100 29v19m-8-24a8 8 0 1 0 16 0 8 8 0 1 0-16 0" className="pet-antenna" />
    <rect x="42" y="48" width="116" height="103" rx="35" className="pet-body" />
    <rect x="54" y="63" width="92" height="61" rx="24" className="robot-screen" />
    <Face energy={energy} y={78} />
    <Meter energy={energy} y={132} />
    <path d="M42 91H27v33m131-33h15v33" className="pet-arm" />
  </svg>;
}
