import { describe, expect, it } from "vitest";
import { kvToText, needsKvQuoting, normalizeKvValue, parseKv, parseList } from "./kv";

describe("parseKv", () => {
  it("parses one KEY=VALUE per line", () => {
    expect(parseKv("API_KEY=sk-abc\nPORT=8080")).toEqual({ API_KEY: "sk-abc", PORT: "8080" });
  });

  it("keeps everything after the first = ", () => {
    expect(parseKv("DSN=postgres://u:p@host/db?a=b")).toEqual({ DSN: "postgres://u:p@host/db?a=b" });
  });

  it("trims the key but not a quoted value", () => {
    // The regression this module exists for: the old parser ran .trim() over the
    // value too, so a credential with edge whitespace was silently rewritten and
    // the server rejected it with nothing in the UI to explain why.
    expect(parseKv('API_KEY="  sk-abc  "')).toEqual({ API_KEY: "  sk-abc  " });
    expect(parseKv('  API_KEY  ="x"')).toEqual({ API_KEY: "x" });
  });

  it("still trims an unquoted value, so a pasted 'KEY = value' does the obvious thing", () => {
    expect(parseKv("API_KEY = sk-abc ")).toEqual({ API_KEY: "sk-abc" });
  });

  it("accepts single quotes as well", () => {
    expect(parseKv("A='  b '")).toEqual({ A: "  b " });
  });

  it("does not strip mismatched or inner quotes", () => {
    expect(parseKv('A="b')).toEqual({ A: '"b' });
    expect(parseKv('A=say "hi"')).toEqual({ A: 'say "hi"' });
  });

  it("skips blank lines and lines with no key", () => {
    expect(parseKv("\n=value\n  \nA=1")).toEqual({ A: "1" });
  });

  it("keeps an explicitly empty value", () => {
    expect(parseKv("A=")).toEqual({ A: "" });
  });
});

describe("normalizeKvValue", () => {
  it("unwraps only a fully matching quote pair", () => {
    expect(normalizeKvValue('"  x  "')).toBe("  x  ");
    expect(normalizeKvValue("'x'")).toBe("x");
    expect(normalizeKvValue("\"x'")).toBe("\"x'");
  });
});

describe("kvToText", () => {
  it("round-trips a value with edge whitespace", () => {
    const record = { API_KEY: "  sk-abc  " };
    expect(needsKvQuoting(record.API_KEY)).toBe(true);
    expect(parseKv(kvToText(record))).toEqual(record);
  });

  it("round-trips a value that already looks quoted", () => {
    const record = { A: '"quoted"' };
    expect(parseKv(kvToText(record))).toEqual(record);
  });

  it("leaves ordinary values unquoted", () => {
    expect(kvToText({ API_KEY: "sk-abc", PORT: "8080" })).toBe("API_KEY=sk-abc\nPORT=8080");
  });

  it("falls back to single quotes when the value contains a double quote", () => {
    expect(parseKv(kvToText({ A: ' say "hi" ' }))).toEqual({ A: ' say "hi" ' });
  });

  it("returns an empty string for no record", () => {
    expect(kvToText(null)).toBe("");
    expect(kvToText(undefined)).toBe("");
  });
});

describe("parseList", () => {
  it("splits on newlines and commas, trimming and de-duplicating", () => {
    expect(parseList("read_file, write_file\nread_file\n\n list_dir ")).toEqual([
      "read_file",
      "write_file",
      "list_dir",
    ]);
  });

  it("returns nothing for a blank block", () => {
    expect(parseList("  \n , ")).toEqual([]);
  });
});
