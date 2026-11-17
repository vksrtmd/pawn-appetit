import { Anchor, Button, Code, CopyButton, Group, Stack, Text, Title } from "@mantine/core";
import { useNavigate } from "@tanstack/react-router";
import { useTranslation } from "react-i18next";

function formatDetail(detail: unknown): string {
  if (detail === null || detail === undefined) return "";
  if (typeof detail === "string") return detail;
  try {
    return JSON.stringify(detail);
  } catch {
    return String(detail);
  }
}

export default function ErrorComponent({ error }: { error: unknown }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const cause = error instanceof Error ? formatDetail(error.cause) : "";

  return (
    <Stack p="md">
      <Title>{t("error.title")}</Title>
      {error instanceof Error ? (
        <>
          <Text>
            <b>{error.name}:</b> {error.message}
          </Text>
          <Code>{error.stack}</Code>
          {cause && <Code>{cause}</Code>}
        </>
      ) : (
        <Text>
          <b>{t("error.unexpectedError")}</b> {formatDetail(error)}
        </Text>
      )}
      <Group>
        {error instanceof Error && (
          <CopyButton value={`${error.message}\n${error.stack}`}>
            {({ copied, copy }) => (
              <Button color={copied ? "teal" : undefined} onClick={copy}>
                {copied ? t("common.copied") : t("error.copyStackTrace")}
              </Button>
            )}
          </CopyButton>
        )}
        <Button onClick={() => navigate({ to: "/" }).then(() => window.location.reload())}>
          {t("common.reload")}
        </Button>
      </Group>

      <Text>
        {t("error.reportPrompt")}{" "}
        <Anchor
          href="https://github.com/Pawn-Appetit/pawn-appetit/issues/new?assignees=&labels=bug&projects=&template=bug.yml"
          target="_blank"
        >
          Github
        </Anchor>{" "}
        {t("error.reportOr")}{" "}
        <Anchor href="https://discord.gg/8hk49G8ZbX" target="_blank">
          Discord server
        </Anchor>
      </Text>
    </Stack>
  );
}
