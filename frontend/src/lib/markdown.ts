import { Marked } from "marked";

/** Markdown HTML is sanitized separately immediately before entering the DOM. */
export function parseMarkdown(source: string): string {
  const parser = new Marked({
    gfm: true,
    renderer: {
      html() { return ""; },
      image({ text }) { return this.parser.parseInline([{ type: "text", raw: text, text }]); },
    },
  });
  return parser.parse(source, { async: false });
}
