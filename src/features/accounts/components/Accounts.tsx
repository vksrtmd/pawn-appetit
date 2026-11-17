import { notifications } from "@mantine/notifications";
import { IconX } from "@tabler/icons-react";
import { listen } from "@tauri-apps/api/event";
import { useAtom } from "jotai";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { DatabaseInfo } from "@/bindings";
import { commands } from "@/bindings";
import type { SortState } from "@/components/GenericHeader";
import { sessionsAtom } from "@/state/atoms";
import { getChessComAccount } from "@/utils/chess.com/api";
import { getDatabases } from "@/utils/db";
import { getLichessAccount } from "@/utils/lichess/api";
import type { ChessComSession, LichessSession } from "@/utils/session";
import AccountModal from "./modals/AccountModal";
import AccountCards from "./views/AccountCards";
import AccountsTableView from "./views/AccountsTableView";

function Accounts({
  open,
  setOpen,
  view,
  query,
  sortBy,
  isLoading = false,
}: {
  open: boolean;
  setOpen: (open: boolean) => void;
  view: "grid" | "table";
  query: string;
  sortBy: SortState;
  isLoading?: boolean;
}) {
  const [, setSessions] = useAtom(sessionsAtom);
  const { t } = useTranslation();
  const [databases, setDatabases] = useState<DatabaseInfo[]>([]);
  useEffect(() => {
    getDatabases().then((dbs) => setDatabases(dbs));
  }, []);

  const addChessComSession = useCallback(
    (alias: string, session: ChessComSession) => {
      setSessions((sessions) => {
        const newSessions = sessions.filter((s) => s.chessCom?.username !== session.username);
        return [
          ...newSessions,
          {
            chessCom: session,
            player: alias,
            updatedAt: Date.now(),
          },
        ];
      });
    },
    [setSessions],
  );

  const addLichessSession = useCallback(
    (alias: string, session: LichessSession) => {
      setSessions((sessions) => {
        const newSessions = sessions.filter((s) => s.lichess?.username !== session.username);
        return [
          ...newSessions,
          {
            lichess: session,
            player: alias,
            updatedAt: Date.now(),
          },
        ];
      });
    },
    [setSessions],
  );

  async function addChessCom(player: string, username: string) {
    const p = player !== "" ? player : username;
    const stats = await getChessComAccount(username);
    if (!stats) {
      return;
    }
    addChessComSession(p, { username, stats });
  }

  async function addLichessNoLogin(player: string, username: string) {
    const p = player !== "" ? player : username;
    const account = await getLichessAccount({ username });
    if (!account) return;
    addLichessSession(p, { username, account });
  }

  const onLichessAuthentication = useCallback(
    async (token: string) => {
      const player = sessionStorage.getItem("lichess_player_alias") || "";
      sessionStorage.removeItem("lichess_player_alias");
      const account = await getLichessAccount({ token });
      if (!account) return;
      const username = account.username;
      const p = player !== "" ? player : username;
      addLichessSession(p, { accessToken: token, username: username, account });
    },
    [addLichessSession],
  );

  const showLoginError = useCallback(
    (message: string) => {
      sessionStorage.removeItem("lichess_player_alias");
      notifications.show({ title: t("common.error"), message, color: "red", icon: <IconX /> });
    },
    [t],
  );

  async function addLichess(player: string, username: string, withLogin: boolean) {
    if (withLogin) {
      sessionStorage.setItem("lichess_player_alias", player);
      const result = await commands.authenticate(username);
      if (result.status === "error") showLoginError(result.error);
      return;
    }
    return await addLichessNoLogin(player, username);
  }

  useEffect(() => {
    const unlisteners: Array<() => void> = [];
    let cancelled = false;
    const add = (p: Promise<() => void>) =>
      p.then((un) => {
        if (cancelled) un();
        else unlisteners.push(un);
      });

    add(listen<string>("access_token", (event) => onLichessAuthentication(event.payload)));
    add(listen<string>("oauth_error", (event) => showLoginError(event.payload)));

    return () => {
      cancelled = true;
      for (const un of unlisteners) un();
    };
  }, [onLichessAuthentication, showLoginError]);

  return (
    <>
      {view === "grid" ? (
        <AccountCards
          databases={databases}
          setDatabases={setDatabases}
          query={query}
          sortBy={sortBy}
          isLoading={isLoading}
          onAddAccount={() => setOpen(true)}
        />
      ) : (
        <AccountsTableView
          databases={databases}
          setDatabases={setDatabases}
          query={query}
          sortBy={sortBy}
          isLoading={isLoading}
          onAddAccount={() => setOpen(true)}
        />
      )}
      <AccountModal
        open={open}
        setOpen={setOpen}
        addLichess={addLichess}
        addChessCom={addChessCom}
      />
    </>
  );
}

export default Accounts;
