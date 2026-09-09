import assert from "node:assert/strict";
import { test } from "node:test";
import { documentFormat, documentPathKey, DOCUMENT_EXTENSIONS } from "../src/lib/documentTypes.ts";

test("only PDF is accepted by the file picker and path routing", () => {
  assert.deepEqual(DOCUMENT_EXTENSIONS, ["pdf"]);
  for (const extension of ["pdf", "PDF", "PdF"]) assert.equal(documentFormat(`E:\\folder\\file.${extension}`), "pdf");
  for (const extension of ["md", "markdown", "mmd", "mermaid", "puml", "plantuml", "pu", "uml", "txt"]) {
    assert.equal(documentFormat(`file.${extension}`), null);
    assert.equal(documentFormat(`file.${extension.toUpperCase()}`), null);
  }
  for (const path of ["C:/pdf", "C:/folder.pdf/file", "x.pdf.exe"]) assert.equal(documentFormat(path), null);
});

test("Windows verbatim paths and UNC aliases share a tab identity", () => {
  assert.equal(documentPathKey("\\\\?\\C:\\Documents\\Notes.PDF"), documentPathKey("c:/documents/notes.pdf"));
  assert.equal(documentPathKey("\\\\?\\UNC\\Server\\Share\\x.pdf"), documentPathKey("//server/share/X.PDF"));
});
