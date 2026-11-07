import { useCallback, useEffect, useState } from "preact/hooks";
import type { Controller } from "../controller.ts";
import { useStore } from "../hooks.ts";
import { approveProposal, listProposals, rejectProposal, reopenProposal, undoLast, type Proposal } from "./api.ts";
import { sortProposals } from "./view.ts";
import { useUndo } from "./useUndo.ts";

const message = (e: unknown): string => (e instanceof Error ? e.message : String(e));

/** 提案の一覧。つながったとき、SSE の `proposal` で読み直す。 */
const useProposalList = ({ api, connection }: Controller) => {
  const link = useStore(connection.subscribe, connection.get).state;
  const [items, setItems] = useState<Proposal[]>([]);
  const [error, setError] = useState<string | null>(null);
  const load = useCallback(async () => {
    try {
      setItems(sortProposals(await listProposals(api)));
    } catch (e) {
      setError(message(e));
    }
  }, [api]);
  useEffect(() => {
    if (link === "online") void load();
  }, [link, load]);
  useEffect(() => connection.onEvent("proposal", () => void load()), [connection, load]);
  return { items, error, setError, load };
};

/**
 * 提案の一覧と、決める操作。確認の画面は出さず、決めた直後の 5 秒だけ `Undo` を出す。
 * 承認の `Undo` は全体の直前のときだけ効き、効かなければ母艦の理由を出す。
 */
export const useProposals = (controller: Controller) => {
  const { api } = controller;
  const { items, error, setError, load } = useProposalList(controller);
  const undo = useUndo();
  const decide = async (call: () => Promise<unknown>, label: string, revert: () => Promise<unknown>): Promise<boolean> => {
    try {
      await call();
      setError(null);
      undo.show({ label, run: async () => void (await revert()) });
      await load();
      return true;
    } catch (e) {
      setError(message(e));
      return false;
    }
  };
  const undoNow = async (): Promise<void> => {
    const current = undo.offer;
    undo.dismiss();
    try {
      await current?.run();
    } catch (e) {
      setError(message(e));
    }
    await load();
  };
  return {
    items,
    error,
    offer: undo.offer,
    undo: undoNow,
    approve: (p: Proposal, ops: number[] | null) =>
      decide(() => approveProposal(api, p.id, ops), `Approved ${p.name}`, () => undoLast(api)),
    reject: (p: Proposal) => decide(() => rejectProposal(api, p.id), `Rejected ${p.name}`, () => reopenProposal(api, p.id)),
    reopen: (p: Proposal) => decide(() => reopenProposal(api, p.id), `Reopened ${p.name}`, () => rejectProposal(api, p.id)),
  };
};
