/** Text color class for an HTTP method badge, so methods are recognizable at a glance. */
export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-emerald-600 dark:text-emerald-400";
    case "POST":
      return "text-amber-600 dark:text-amber-400";
    case "PUT":
    case "PATCH":
      return "text-sky-600 dark:text-sky-400";
    case "DELETE":
      return "text-rose-600 dark:text-rose-400";
    default:
      return "text-violet-600 dark:text-violet-400";
  }
}
