import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CodeEditor } from "./CodeEditor";

describe("CodeEditor", () => {
  afterEach(cleanup);

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
