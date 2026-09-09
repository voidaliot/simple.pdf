import DOMPurify from "dompurify";

export function sanitizeMarkdown(html: string): string {
  return DOMPurify.sanitize(html, {
    USE_PROFILES: { html: true },
    FORBID_TAGS: ["style", "form", "input", "button", "textarea", "select", "img", "video", "audio"],
    FORBID_ATTR: ["style", "id", "name"],
  });
}
