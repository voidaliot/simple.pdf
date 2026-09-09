export type DocumentFormat = "pdf";

export const DOCUMENT_EXTENSIONS = ["pdf"];

export function documentFormat(path: string): DocumentFormat | null {
  const extension = path.split(/[\\/]/).pop()?.split(".").slice(1).pop()?.toLowerCase();
  switch (extension) {
    case "pdf": return "pdf";
    default: return null;
  }
}

/** Canonical paths returned by Windows can use verbatim drive or UNC prefixes. */
export function documentPathKey(path: string): string {
  const normalized = path.replaceAll("/", "\\").toLowerCase();
  if (normalized.startsWith("\\\\?\\unc\\")) return `\\\\${normalized.slice(8)}`;
  return normalized.startsWith("\\\\?\\") ? normalized.slice(4) : normalized;
}
