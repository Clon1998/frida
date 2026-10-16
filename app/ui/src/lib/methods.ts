/** The standard HTTP methods offered in the method dropdown. */
export const METHODS = ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS"];

/** Text color class for an HTTP method badge, so methods are recognizable at a glance. */
export function methodColor(method: string): string {
  switch (method.toUpperCase()) {
    case "GET":
      return "text-success";
    case "POST":
      return "text-warning";
    case "PUT":
    case "PATCH":
      return "text-info";
    case "DELETE":
      return "text-destructive";
    default:
      return "text-violet-600 dark:text-violet-400";
  }
}
