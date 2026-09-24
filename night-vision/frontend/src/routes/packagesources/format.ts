const DAY_MS = 24 * 60 * 60 * 1000;

const startOfDay = (date: Date): number =>
 new Date(date.getFullYear(), date.getMonth(), date.getDate()).getTime();

// "Today, 9:11 AM", "Yesterday, 4:02 PM", or "Sep 3, 9:11 AM", like the example pages.
export const formatTimestamp = (value: string | null | undefined): string => {
 if (!value) return "—";
 const date = new Date(value);
 const now = new Date();
 const daysAgo = Math.round((startOfDay(now) - startOfDay(date)) / DAY_MS);
 const day =
  daysAgo === 0
   ? "Today"
   : daysAgo === 1
    ? "Yesterday"
    : date.toLocaleDateString(undefined, {
       month: "short",
       day: "numeric",
       year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
      });
 return `${day}, ${date.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })}`;
};

// "dependency-unavailable" -> "Dependency unavailable"
export const formatFailureKind = (value: string | null | undefined): string =>
 value ? value.charAt(0).toUpperCase() + value.slice(1).replaceAll("-", " ") : "—";

// "pkg:npm/%40scope/name@1.2.3" -> "@scope/name@1.2.3"
export const formatPurl = (purl: string): string =>
 decodeURIComponent(purl.replace(/^pkg:npm\//, ""));
