import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CodeEditor } from "./CodeEditor";

describe("CodeEditor", () => {
  afterEach(cleanup);

  // Regression: every editor put a full-height popup container into <body>, so the window
  // could be scrolled below the app.
  it("keeps popup containers in one zero-size host instead of <body>", () => {
    render(
      <>
        <CodeEditor value="a" singleLine />
        <CodeEditor value="b" singleLine />
      </>,
    );
    const host = document.getElementById("cm-tooltip-host");
    expect(host?.parentElement).toBe(document.body);
    expect(host?.style.height).toBe("0px");
    expect(host?.children.length).toBe(2);
    const strays = [...document.body.children].filter((e) => e.className.includes("ͼ"));
    expect(strays).toEqual([]);
  });

  // Regression: switching the content of a preview tab replaced the editor text from outside,
  // which was reported as a user edit and pinned the preview tab.
  it("does not report text replaced from outside as an edit", () => {
    const onChange = vi.fn();
    const { rerender } = render(<CodeEditor value="first" onChange={onChange} singleLine />);
    rerender(<CodeEditor value="second" onChange={onChange} singleLine />);
    expect(onChange).not.toHaveBeenCalled();
    expect(document.querySelector(".cm-content")?.textContent).toBe("second");
  });
});
