import { beforeEach, describe, expect, it } from "vitest";
import {
  getStoredZoom,
  setZoomLevel,
  ZOOM_OPTIONS,
  initZoom,
} from "./zoom";

describe("zoom", () => {
  beforeEach(() => {
    localStorage.clear();
    document.documentElement.style.zoom = "";
  });

  it("provides exactly the four requested zoom options", () => {
    expect(ZOOM_OPTIONS).toHaveLength(4);
    expect(ZOOM_OPTIONS.map((o) => o.value)).toEqual(["90", "100", "110", "125"]);
    expect(ZOOM_OPTIONS.map((o) => o.factor)).toEqual([0.9, 1.0, 1.1, 1.25]);
  });

  it("defaults to 100 when nothing stored", () => {
    expect(getStoredZoom()).toBe("100");
  });

  it("reads stored valid zoom level", () => {
    localStorage.setItem("z-ffmpeg-ui-zoom", "125");
    expect(getStoredZoom()).toBe("125");

    localStorage.setItem("z-ffmpeg-ui-zoom", "90");
    expect(getStoredZoom()).toBe("90");
  });

  it("falls back to 100 for invalid stored value", () => {
    localStorage.setItem("z-ffmpeg-ui-zoom", "invalid");
    expect(getStoredZoom()).toBe("100");
  });

  it("updates stored level and applies zoom", () => {
    setZoomLevel("110");
    expect(getStoredZoom()).toBe("110");
    expect(localStorage.getItem("z-ffmpeg-ui-zoom")).toBe("110");
    expect(document.documentElement.style.zoom).toBe("1.1");
  });

  it("initZoom initializes zoom and sets style", () => {
    localStorage.setItem("z-ffmpeg-ui-zoom", "125");
    const level = initZoom();
    expect(level).toBe("125");
    expect(document.documentElement.style.zoom).toBe("1.25");
  });

  it("blocks zoom hotkey events on window", () => {
    initZoom();

    let prevented = false;
    const event = new KeyboardEvent("keydown", {
      key: "=",
      ctrlKey: true,
      cancelable: true,
    });

    Object.defineProperty(event, "preventDefault", {
      value: () => {
        prevented = true;
      },
    });

    window.dispatchEvent(event);
    expect(prevented).toBe(true);
  });
});
