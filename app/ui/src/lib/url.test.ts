import { describe, expect, it } from "vitest";
import {
  parseQuery,
  pathParamNames,
  queryParamsFromUrl,
  syncPathParams,
  urlWithQueryParams,
} from "./url";

const row = (name: string, value: string, enabled = true) => ({ name, value, enabled });

describe("query params", () => {
  it("parses raw pairs without decoding", () => {
    expect(parseQuery("http://x/a?expand=items&q={{term}}&flag&enc=a%20b#top")).toEqual([
      { name: "expand", value: "items" },
      { name: "q", value: "{{term}}" },
      { name: "flag", value: "" },
      { name: "enc", value: "a%20b" },
    ]);
    expect(parseQuery("http://x/a")).toEqual([]);
  });

  it("keeps disabled rows when the URL changes", () => {
    const previous = [row("a", "1"), row("off", "x", false)];
    expect(queryParamsFromUrl("http://x?a=2&b=3", previous)).toEqual([
      row("a", "2"),
      row("b", "3"),
      row("off", "x", false),
    ]);
  });

  it("writes only enabled, named rows back into the URL", () => {
    const params = [row("a", "1"), row("off", "x", false), row("", "ignored"), row("flag", "")];
    expect(urlWithQueryParams("http://x/p?old=1#frag", params)).toBe("http://x/p?a=1&flag#frag");
    expect(urlWithQueryParams("http://x/p?old=1", [])).toBe("http://x/p");
  });

  it("round-trips URL -> params -> URL", () => {
    const url = "{{baseUrl}}/orders?expand=items&limit={{limit}}";
    expect(urlWithQueryParams(url, queryParamsFromUrl(url, []))).toBe(url);
  });
});

describe("path params", () => {
  it("finds :params in the path but not the port", () => {
    expect(pathParamNames("http://localhost:8080/orders/:orderId/items/:itemId?x=:no")).toEqual([
      "orderId",
      "itemId",
    ]);
    expect(pathParamNames("{{baseUrl}}/orders/:id")).toEqual(["id"]);
    expect(pathParamNames("localhost:3000")).toEqual([]);
  });

  it("keeps entered values for params that still exist", () => {
    const previous = [
      { name: "orderId", value: "42" },
      { name: "gone", value: "x" },
    ];
    expect(syncPathParams("/orders/:orderId/:itemId", previous)).toEqual([
      { name: "orderId", value: "42" },
      { name: "itemId", value: "" },
    ]);
  });
});
