import assert from "node:assert/strict";
import { test } from "node:test";
import { addRecent, loadRecents, persistRecents, type RecentEntry } from "../src/lib/recentFiles.ts";

const entry: RecentEntry = { path: "C:\\diagram.mmd", title: "diagram", lastOpened: 1, pinned: true };

test("corrupt or unexpected recent storage cannot crash the home screen", () => {
  for (const raw of ["null", "{}", "bad JSON", "[null, 12, {}]"]) assert.deepEqual(loadRecents(raw), []);
  assert.equal(loadRecents(JSON.stringify([entry, { ...entry, path: "c:/DIAGRAM.mmd" }])).length, 1);
  assert.equal(loadRecents(JSON.stringify([{ ...entry, thumbnail: "https://example.com/track" }]))[0]?.thumbnail, undefined);
});

test("reopening aliases preserves the existing pin and thumbnail", () => {
  const result = addRecent([{ ...entry, thumbnail: "thumb" }], "c:/DIAGRAM.mmd", "renamed", 2);
  assert.equal(result.length, 1);
  assert.equal(result[0]?.pinned, true);
  assert.equal(result[0]?.thumbnail, "thumb");
  assert.equal(result[0]?.lastOpened, 2);
});

test("the recent limit evicts an unpinned file while preserving older pins", () => {
  const entries = Array.from({ length: 50 }, (_, i) => ({ ...entry, path: `C:\\${i}.pdf`, pinned: i === 49 }));
  const result = addRecent(entries, "C:\\new.mmd", "new", 2);
  assert.equal(result.length, 50);
  assert.ok(result.some((item) => item.path === "C:\\49.pdf"));
  assert.ok(!result.some((item) => item.path === "C:\\48.pdf"));
});

test("quota failures retry without thumbnails and never interrupt document opening", () => {
  const calls: string[] = [];
  persistRecents({ setItem(_key, value) { calls.push(value); if (calls.length === 1) throw new Error("Quota exceeded"); } }, "recents", [{ ...entry, thumbnail: "large thumbnail" }]);
  assert.equal(calls.length, 2);
  assert.doesNotMatch(calls[1]!, /thumbnail/);
  assert.doesNotThrow(() => persistRecents({ setItem() { throw new Error("Storage disabled"); } }, "recents", [entry]));
});
