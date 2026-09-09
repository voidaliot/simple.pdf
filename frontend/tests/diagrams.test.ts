import assert from "node:assert/strict";
import { test } from "node:test";
import { documentFormat, documentPathKey, DOCUMENT_EXTENSIONS } from "../src/lib/documentTypes.ts";
import { parseTextDocument, prepareDiagram } from "../src/lib/diagramSource.ts";

test("recognizes every open-dialog extension with mixed casing", () => {
  for (const extension of DOCUMENT_EXTENSIONS) assert.ok(documentFormat(`E:\\folder\\FILE.${extension.toUpperCase()}`));
  for (const path of ["C:/mmd", "C:/folder.md/file", "x.puml.exe", "file.txt"]) assert.equal(documentFormat(path), null);
});

test("Windows verbatim paths and UNC aliases share a tab identity", () => {
  assert.equal(documentPathKey("\\\\?\\C:\\Diagrams\\Flow.MMD"), documentPathKey("c:/diagrams/flow.mmd"));
  assert.equal(documentPathKey("\\\\?\\UNC\\Server\\Share\\x.puml"), documentPathKey("//server/share/X.PUML"));
});

test("renders backtick, tilde, mixed-case and nested diagram fences with surrounding Markdown", () => {
  const parsed = parseTextDocument("markdown", "# Overview\n\n```Mermaid title\ngraph TD\nA-->B\n```\n\n> ~~~puml\n> Alice -> Bob\n> ~~~\n\n- Nested:\n\n  ```uml\n  Bob -> Alice\n  ```\n\n```js\nconst x = 1;\n```\n");
  assert.deepEqual(parsed.diagrams.map((item) => item.format), ["mermaid", "plantuml", "plantuml"]);
  assert.match(parsed.html, /<h1>Overview<\/h1>/);
  assert.match(parsed.html, /const x = 1/);
  assert.equal((parsed.html.match(/data-diagram-index=/g) ?? []).length, 3);
});

test("raw HTML cannot inject diagram slots or executable elements", () => {
  const parsed = parseTextDocument("markdown", '<div data-diagram-index="999"><script>alert(1)</script></div>\n\n```html\n<script>alert(2)</script>\n```');
  assert.equal(parsed.diagrams.length, 0);
  assert.doesNotMatch(parsed.html, /<script|data-diagram-index/);
  assert.match(parsed.html, /&lt;script&gt;/);
});

test("multiple standalone PlantUML diagrams are all retained", () => {
  const parsed = parseTextDocument("plantuml", "@startuml\nAlice -> Bob\n@enduml\n\n@startmindmap\n* Root\n** Child\n@endmindmap");
  assert.equal(parsed.diagrams.length, 2);
  assert.match(parsed.diagrams[1]!.source, /@startmindmap/);
  assert.equal(prepareDiagram("plantuml", "Alice -> Bob"), "@startuml\nAlice -> Bob\n@enduml");
});

test("PlantUML ignores text outside diagram markers, including commented examples", () => {
  const source = "A document preamble\n/'\n@startuml\nnot a diagram\n@enduml\n'/\n' header\n@startuml\nAlice -> Bob\n@enduml\nText between diagrams\n@startwbs\n* Root\n@endwbs\nFooter";
  const parsed = parseTextDocument("plantuml", source);
  assert.deepEqual(parsed.diagrams.map((diagram) => diagram.source), ["@startuml\nAlice -> Bob\n@enduml", "@startwbs\n* Root\n@endwbs"]);
  assert.equal(prepareDiagram("plantuml", "' heading\n@startuml\nA -> B\n@enduml\ntrailing text"), "@startuml\nA -> B\n@enduml");
});

test("commented includes do not block a self-contained diagram", () => {
  assert.doesNotThrow(() => prepareDiagram("plantuml", "@startuml\n/'\n!include https://example.com/file\n'/\nA -> B\n@enduml"));
});

test("unsupported browser-engine features are identified as renderer limitations", () => {
  for (const source of ["@startditaa\n+--+\n@endditaa", "@startsalt\n{ [Button] }\n@endsalt", "@startuml\nnwdiag {\n}\n@enduml", "@startuml\nlistopeniconic\n@enduml"]) {
    assert.throws(() => prepareDiagram("plantuml", source), /not supported by the bundled PlantUML JavaScript engine/);
  }
});

test("bounds diagram counts and source sizes before loading an engine", () => {
  assert.throws(() => prepareDiagram("mermaid", " "), /empty/);
  assert.throws(() => prepareDiagram("plantuml", "A".repeat(50_001)), /50,000/);
  assert.throws(() => parseTextDocument("markdown", "```mmd\ngraph TD\nA-->B\n```\n".repeat(101)), /100 diagrams/);
});

test("network includes and external data receive an explicit offline error", () => {
  for (const source of ["!include https://example.com/a", "!include_once <aws>", "!import x", "%load_json(\"file\")", "!theme blue from https://example.com"]) {
    assert.throws(() => prepareDiagram("plantuml", source), /unavailable offline/);
  }
  assert.match(prepareDiagram("plantuml", "@startuml\n!define NAME Alice\nNAME -> Bob\n@enduml"), /!define/);
});
