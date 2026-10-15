/**
 * Named environment colors (stored as the name in the environment file). Full class strings
 * are listed literally so Tailwind keeps them in the build.
 */
export interface EnvColor {
  name: string;
  /** Solid swatch / dot. */
  dot: string;
  /** Tinted button for the environment picker. */
  button: string;
  /** Accent line under the header bar. */
  line: string;
}

export const ENV_COLORS: EnvColor[] = [
  {
    name: "red",
    dot: "bg-red-500",
    button: "border-red-500 bg-red-500/15 text-red-700 dark:text-red-300",
    line: "border-b-red-500",
  },
  {
    name: "orange",
    dot: "bg-orange-500",
    button: "border-orange-500 bg-orange-500/15 text-orange-700 dark:text-orange-300",
    line: "border-b-orange-500",
  },
  {
    name: "amber",
    dot: "bg-amber-500",
    button: "border-amber-500 bg-amber-500/15 text-amber-700 dark:text-amber-300",
    line: "border-b-amber-500",
  },
  {
    name: "green",
    dot: "bg-emerald-500",
    button: "border-emerald-500 bg-emerald-500/15 text-emerald-700 dark:text-emerald-300",
    line: "border-b-emerald-500",
  },
  {
    name: "teal",
    dot: "bg-teal-500",
    button: "border-teal-500 bg-teal-500/15 text-teal-700 dark:text-teal-300",
    line: "border-b-teal-500",
  },
  {
    name: "blue",
    dot: "bg-blue-500",
    button: "border-blue-500 bg-blue-500/15 text-blue-700 dark:text-blue-300",
    line: "border-b-blue-500",
  },
  {
    name: "violet",
    dot: "bg-violet-500",
    button: "border-violet-500 bg-violet-500/15 text-violet-700 dark:text-violet-300",
    line: "border-b-violet-500",
  },
  {
    name: "pink",
    dot: "bg-pink-500",
    button: "border-pink-500 bg-pink-500/15 text-pink-700 dark:text-pink-300",
    line: "border-b-pink-500",
  },
  {
    name: "gray",
    dot: "bg-zinc-500",
    button: "border-zinc-500 bg-zinc-500/15 text-zinc-700 dark:text-zinc-300",
    line: "border-b-zinc-500",
  },
];

/** The color for a stored name, or `null` for none/unknown. */
export function envColor(name: string | null | undefined): EnvColor | null {
  return ENV_COLORS.find((c) => c.name === name) ?? null;
}
