import { h } from "preact";
import { definePanel } from "@mm/sdk";
import { createIpcDiagnosticsApi } from "./api.ts";
import { DiagnosticsPanel } from "./DiagnosticsPanel.tsx";

export const panel = definePanel({
  id: "diagnostics",
  label: "Diagnostics",
  column: "right",
  order: 100,
  createApi: createIpcDiagnosticsApi,
  render: ({ api, active, visits, requests, docChanges }) =>
    h(DiagnosticsPanel, { api, active, refreshKey: visits + requests + docChanges }),
});
