export type DiagramFormat = "mermaid" | "plantuml";
export type TextFormat = DiagramFormat | "markdown";
export type DocumentFormat = "pdf" | TextFormat;

export const DOCUMENT_EXTENSIONS = ["pdf", "mmd", "mermaid", "puml", "plantuml", "pu", "uml", "md", "markdown"];

export function documentFormat(path: string): DocumentFormat | null {
  const extension = path.split(/[\\/]/).pop()?.split(".").slice(1).pop()?.toLowerCase();
  switch (extension) {
    case "pdf": return "pdf";
    case "mmd": case "mermaid": return "mermaid";
    case "puml": case "plantuml": case "pu": case "uml": return "plantuml";
    case "md": case "markdown": return "markdown";
    default: return null;
  }
}

/** Canonical paths returned by Windows can use verbatim drive or UNC prefixes. */
export function documentPathKey(path: string): string {
  const normalized = path.replaceAll("/", "\\").toLowerCase();
  if (normalized.startsWith("\\\\?\\unc\\")) return `\\\\${normalized.slice(8)}`;
  return normalized.startsWith("\\\\?\\") ? normalized.slice(4) : normalized;
}

export function diagramLanguage(info: string): DiagramFormat | null {
  switch (info.trim().split(/\s+/)[0]?.toLowerCase()) {
    case "mermaid": case "mmd": return "mermaid";
    case "plantuml": case "puml": case "pu": case "uml": return "plantuml";
    default: return null;
  }
}
