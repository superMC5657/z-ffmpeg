import { beforeEach, describe, expect, it, vi } from "vitest";
import { useLicenseStore, FREE_MAX_CONCURRENT } from "@/store/licenseStore";
import type { LicenseStatus } from "@/types";

// mock @/lib/tauri，避免测试触发真实 IPC
vi.mock("@/lib/tauri", () => ({
  getLicenseStatus: vi.fn(),
  activateLicense: vi.fn(),
  deactivateLicense: vi.fn(),
}));

import {
  getLicenseStatus,
  activateLicense,
  deactivateLicense,
} from "@/lib/tauri";

const freeStatus: LicenseStatus = {
  pro: false,
  levelLabel: null,
  email: null,
  expiresAt: null,
  features: [],
  offline: false,
  code: null,
  buyUrl: null,
};

const proStatus: LicenseStatus = {
  pro: true,
  levelLabel: "专业版",
  email: "buyer@example.com",
  expiresAt: "2026-09-14T12:00:00+08:00",
  features: ["pro"],
  offline: false,
  code: "SDX4-K9TP-2M7Q-W3HZ",
  buyUrl: "http://localhost:5173/buy/z-ffmpeg",
};

describe("licenseStore", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useLicenseStore.setState({
      status: null,
      loading: true,
      working: false,
      activationOpen: false,
    });
  });

  it("fetchStatus loads the license status from backend", async () => {
    vi.mocked(getLicenseStatus).mockResolvedValue(proStatus);
    await useLicenseStore.getState().fetchStatus();
    expect(useLicenseStore.getState().status).toEqual(proStatus);
    expect(useLicenseStore.getState().loading).toBe(false);
  });

  it("fetchStatus failure leaves null status (gated as free)", async () => {
    vi.mocked(getLicenseStatus).mockRejectedValue(new Error("ipc down"));
    await useLicenseStore.getState().fetchStatus();
    expect(useLicenseStore.getState().status).toBeNull();
    expect(useLicenseStore.getState().loading).toBe(false);
  });

  it("activate updates status and closes the activation dialog", async () => {
    vi.mocked(activateLicense).mockResolvedValue(proStatus);
    useLicenseStore.setState({ activationOpen: true });

    const status = await useLicenseStore.getState().activate("SDX4-K9TP-2M7Q-W3HZ", "buyer@example.com");

    expect(status.pro).toBe(true);
    expect(useLicenseStore.getState().status?.pro).toBe(true);
    expect(useLicenseStore.getState().activationOpen).toBe(false);
    expect(useLicenseStore.getState().working).toBe(false);
    expect(activateLicense).toHaveBeenCalledWith("SDX4-K9TP-2M7Q-W3HZ", "buyer@example.com");
  });

  it("activate rejection propagates and keeps dialog open", async () => {
    vi.mocked(activateLicense).mockRejectedValue(new Error("激活码与购买邮箱不匹配"));
    useLicenseStore.setState({ activationOpen: true });

    await expect(
      useLicenseStore.getState().activate("XXXX-YYYY-ZZZZ-1111", "wrong@example.com")
    ).rejects.toThrow("激活码与购买邮箱不匹配");

    expect(useLicenseStore.getState().activationOpen).toBe(true);
    expect(useLicenseStore.getState().working).toBe(false);
  });

  it("deactivate downgrades to the returned (free) status", async () => {
    vi.mocked(deactivateLicense).mockResolvedValue(freeStatus);
    useLicenseStore.setState({ status: proStatus });

    const status = await useLicenseStore.getState().deactivate();

    expect(status.pro).toBe(false);
    expect(useLicenseStore.getState().status?.pro).toBe(false);
  });

  it("setActivationOpen toggles the global dialog", () => {
    useLicenseStore.getState().setActivationOpen(true);
    expect(useLicenseStore.getState().activationOpen).toBe(true);
    useLicenseStore.getState().setActivationOpen(false);
    expect(useLicenseStore.getState().activationOpen).toBe(false);
  });

  it("FREE_MAX_CONCURRENT matches the backend constant", () => {
    expect(FREE_MAX_CONCURRENT).toBe(2);
  });
});
