import assert from "node:assert/strict";
import { test } from "node:test";
import { documentFormat, documentPathKey, DOCUMENT_EXTENSIONS } from "../src/lib/documentTypes.ts";
import { parseMarkdown } from "../src/lib/markdown.ts";

test("recognizes PDF and Markdown but rejects experimental diagram extensions", () => {
  for (const extension of DOCUMENT_EXTENSIONS) assert.ok(documentFormat(`E:\\folder\\FILE.${extension.toUpperCase()}`));
  for (const extension of ["mmd", "mermaid", "puml", "plantuml", "pu", "uml"]) {
    assert.equal(documentFormat(`file.${extension}`), null);
    assert.equal(documentFormat(`file.${extension.toUpperCase()}`), null);
  }
  for (const path of ["C:/md", "C:/folder.md/file", "x.puml.exe", "file.txt"]) assert.equal(documentFormat(path), null);
});

test("Windows verbatim paths and UNC aliases share a tab identity", () => {
  assert.equal(documentPathKey("\\\\?\\C:\\Documents\\Notes.MD"), documentPathKey("c:/documents/notes.md"));
  assert.equal(documentPathKey("\\\\?\\UNC\\Server\\Share\\x.pdf"), documentPathKey("//server/share/X.PDF"));
});

test("all former diagram fence aliases remain escaped code, including nested tilde fences", () => {
  for (const language of ["Mermaid", "mmd", "plantuml", "puml", "pu", "uml"]) {
    const html = parseMarkdown(`# Overview\n\n> ~~~${language}\n> Alice -> Bob <script>alert(1)</script>\n> ~~~\n\nAfter.`);
    assert.match(html, /<h1>Overview<\/h1>/);
    assert.match(html, /<blockquote>\n<pre><code/);
    assert.match(html, /Alice -&gt; Bob &lt;script&gt;/);
    assert.match(html, /<p>After\.<\/p>/);
    assert.doesNotMatch(html, /diagram-slot|<svg|<iframe|<script>/);
  }
});

test("Markdown omits raw HTML and remote images while retaining prose", () => {
  const html = parseMarkdown('# Safe\n\n<script>alert(1)</script>\n\n![caption](https://example.com/image)\n\nText.');
  assert.match(html, /<h1>Safe<\/h1>/);
  assert.match(html, /caption/);
  assert.match(html, /Text\./);
  assert.doesNotMatch(html, /<script|<img|https:\/\/example.com/);
});
