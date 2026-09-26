// ABOUTME: Guards production code against stray console.log noise.
// ABOUTME: console.error/warn stay allowed for diagnostics; tests may log.
import { describe, expect, it } from "vitest";
import { readdir, readFile } from "node:fs/promises";
import { join } from "node:path";

const sourceRoot = join(process.cwd(), "src");

async function listFiles(directory: string): Promise<string[]> {
  const entries = await readdir(directory, { withFileTypes: true });
  const files: string[] = [];

  for (const entry of entries) {
    const fullPath = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listFiles(fullPath)));
    } else if (fullPath.endsWith(".vue") || fullPath.endsWith(".ts")) {
      files.push(fullPath);
    }
  }

  return files;
}

describe("no console.log in production code", () => {
  it("has no console.log outside specs and explicit allowlist", async () => {
    const allowlist = new Set<string>();
    const files = await listFiles(sourceRoot);
    const offenders: string[] = [];

    for (const file of files) {
      if (file.includes("__tests__") || file.endsWith(".spec.ts")) {
        continue;
      }
      if (allowlist.has(file)) {
        continue;
      }
      const contents = await readFile(file, "utf-8");
      if (/(^|[^.\w])console\.log\(/.test(contents)) {
        offenders.push(file);
      }
    }

    expect(offenders).toEqual([]);
  });
});
