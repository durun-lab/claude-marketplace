import { h } from "preact";
import { definePanel } from "@mm/sdk";
import { createIpcDeliverApi } from "./api.ts";
import { DeliverPanel } from "./DeliverPanel.tsx";

/** Tasks の書き出しの項目（`export.start`、`export.choose_target`）は Deliver のタブを開く。 */
export const panel = definePanel({
  id: "deliver",
  label: "Deliver",
  column: "left",
  order: 50,
  createApi: createIpcDeliverApi,
  render: ({ api, docChanges }) => h(DeliverPanel, { api, refreshKey: docChanges }),
});
