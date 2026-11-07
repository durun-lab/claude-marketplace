import { h } from "preact";
import { definePanel } from "@mm/sdk";
import { createIpcDevicesApi } from "./api.ts";
import { DevicesPanel } from "./DevicesPanel.tsx";

export const panel = definePanel({
  id: "devices",
  label: "Devices",
  column: "right",
  order: 80,
  createApi: createIpcDevicesApi,
  render: ({ api, visits }) => h(DevicesPanel, { api, refreshKey: visits }),
});
