import { MantineProvider } from "@mantine/core";
import { renderToString } from "react-dom/server";
import { expect, test, vi } from "vitest";
import ErrorComponent from "@/components/ErrorComponent";

vi.mock("@tanstack/react-router", () => ({ useNavigate: () => vi.fn<() => void>() }));
vi.mock("react-i18next", () => ({ useTranslation: () => ({ t: (k: string) => k }) }));

function render(error: unknown) {
  return renderToString(
    <MantineProvider>
      <ErrorComponent error={error} />
    </MantineProvider>,
  );
}

test("error with an object cause renders the cause as text", () => {
  const html = render(new Error("boom", { cause: { code: 42 } }));
  expect(html).toContain("boom");
  expect(html).toContain("{&quot;code&quot;:42}");
});

test("a circular non-Error value still renders the error screen", () => {
  const circular: Record<string, unknown> = {};
  circular.self = circular;
  expect(() => render(circular)).not.toThrow();
});
