import { describe, expect, it } from "vitest";
import type { DatabaseInfo } from "@/bindings";
import { isDefaultDatabaseInstalled } from "../db";

const installed = (filename: string, title: string): DatabaseInfo => ({
  type: "success",
  file: `/data/db/${filename}`,
  filename,
  title,
  description: "",
  player_count: 0,
  event_count: 0,
  game_count: 0,
  storage_size: 0n,
  indexed: false,
});

describe("isDefaultDatabaseInstalled", () => {
  it("shows a downloaded database as installed although its internal title differs", () => {
    const databases = [installed("Lumbra's Gigabase.db3", "LumbrasGigaBase")];
    expect(isDefaultDatabaseInstalled(databases, "Lumbra's Gigabase")).toBe(true);
  });

  it("does not show a database as installed when no file carries its name", () => {
    const databases = [installed("MillionBase.db3", "MillionBase")];
    expect(isDefaultDatabaseInstalled(databases, "Caissabase 2024")).toBe(false);
  });

  it("ignores databases that failed to load", () => {
    const databases = [
      { type: "error", file: "/data/db/Caissabase 2024.db3", error: "bad" } as DatabaseInfo,
    ];
    expect(isDefaultDatabaseInstalled(databases, "Caissabase 2024")).toBe(false);
  });
});
