import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useToastStore } from "@/store/toastStore";

describe("toastStore", () => {
  beforeEach(() => {
    useToastStore.setState({ toasts: [] });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("adds a toast and auto-dismisses it after 3.2s", () => {
    vi.useFakeTimers();
    useToastStore.getState().showToast("搞定", "success");
    expect(useToastStore.getState().toasts).toHaveLength(1);
    expect(useToastStore.getState().toasts[0].type).toBe("success");

    vi.advanceTimersByTime(3200);
    expect(useToastStore.getState().toasts).toHaveLength(0);
  });

  it("dismissToast removes a specific toast by id", () => {
    useToastStore.getState().showToast("a", "info");
    useToastStore.getState().showToast("b", "info");
    const [first] = useToastStore.getState().toasts;
    useToastStore.getState().dismissToast(first.id);
    expect(useToastStore.getState().toasts.map((t) => t.message)).toEqual(["b"]);
  });
});
