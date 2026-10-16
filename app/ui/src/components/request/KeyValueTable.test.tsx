import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
});
