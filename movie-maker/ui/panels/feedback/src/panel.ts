import { h } from "preact";
import { definePanel } from "@mm/sdk";
import { createIpcFeedbackApi } from "./api.ts";
import { FeedbackPanel } from "./FeedbackPanel.tsx";
import { reportWindowErrors } from "./reportErrors.ts";

/** API を作るのは Shell の起動の 1 回なので、ここで画面の例外の見張りも始める（どのタブが開いていても届く）。 */
const createApi = () => {
  const api = createIpcFeedbackApi();
  reportWindowErrors(window, api.reportError);
  return api;
};

export const panel = definePanel({
  id: "feedback",
  label: "Feedback",
  column: "right",
  order: 90,
  createApi,
  render: ({ api, visits }) => h(FeedbackPanel, { api, refreshKey: visits }),
});
