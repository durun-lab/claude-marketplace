import { render } from "preact";
import { closeShortcutsWindow, createIpcCommandsApi } from "@mm/sdk";
import { followTheme } from "../../shell/src/followTheme.ts";
import { Shortcuts } from "./Shortcuts.tsx";
import "./shortcuts.css";

followTheme(document.documentElement, location.search, matchMedia.bind(window));
const root = document.getElementById("root");
if (root) render(<Shortcuts commands={createIpcCommandsApi()} close={() => closeShortcutsWindow()} />, root);
