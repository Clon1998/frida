import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { KeyValueTable } from "./KeyValueTable";

describe("key/value table", () => {
  afterEach(cleanup);

  it("offers a drag handle per row (not for the add row) and toggles rows", () => {
    const onChange = vi.fn();
    const rows = [
      { name: "a", value: "1", enabled: true },
      { name: "b", value: "2", enabled: true },
    ];
    render(<KeyValueTable rows={rows} onChange={onChange} />);
    expect(screen.getAllByLabelText("Drag to reorder")).toHaveLength(2);

    fireEvent.click(screen.getAllByLabelText("Enabled")[1] as HTMLElement);
    expect(onChange).toHaveBeenCalledWith([rows[0], { ...rows[1], enabled: false }]);
  });

  it("keeps typing in the same row after the add row turns into a real row", () => {
    function Stateful() {
      const [rows, setRows] = useState<{ name: string; value: string; enabled: boolean }[]>([]);
      return <KeyValueTable rows={rows} onChange={setRows} />;
    }
    render(<Stateful />);
    const input = screen.getByPlaceholderText(/^Add /) as HTMLInputElement;
    input.focus();
    fireEvent.change(input, { target: { value: "a" } });
    // Same element, still focused: the next keystroke edits this row instead of adding another.
    expect(document.activeElement).toBe(input);
    fireEvent.change(input, { target: { value: "ab" } });
    expect(screen.getAllByLabelText("Drag to reorder")).toHaveLength(1);
    expect(input.value).toBe("ab");
  });
});
