import { describe, expect, it } from "vitest";
import type { Engine } from "@/utils/engines";
import { listenableEngines } from "../EvalListener";

const engine = (name: string, loaded?: boolean | null) =>
    ({ type: "local", name, loaded }) as unknown as Engine;

describe("listenableEngines", () => {
    it("only includes loaded engines, so an unloaded engine is never started again", () => {
        const engines = [engine("a", true), engine("b", false), engine("c"), engine("d", null)];
        expect(listenableEngines(engines).map((e) => e.name)).toEqual(["a"]);
    });
});
