import { describe, expect, it } from "vitest";
import { startPlayback, type MediaLike } from "./usePlayer.ts";

const fakeAudio = (attr: string | null, readyState: number) => {
  const calls: string[] = [];
  const el: MediaLike = {
    src: attr ?? "",
    currentTime: 0,
    readyState,
    getAttribute: () => attr,
    play: async () => void calls.push("play"),
  };
  return { el, calls };
};

describe("素材の音を鳴らし始める", () => {
  it("初めての音は読み込みを待たずに play を呼び、始めの位置はメディアフラグメントで渡す", async () => {
    const { el, calls } = fakeAudio(null, 0);
    const started = startPlayback(el, "/m/api/audio/a1.m4a", 3.5);
    expect(calls).toEqual(["play"]);
    expect(el.src).toBe("/m/api/audio/a1.m4a#t=3.5");
    await started;
  });

  it("読み込み済みの同じ音はシークして鳴らす", async () => {
    const { el, calls } = fakeAudio("/m/api/audio/a1.m4a#t=1", 1);
    await startPlayback(el, "/m/api/audio/a1.m4a", 7);
    expect(el.currentTime).toBe(7);
    expect(el.src).toBe("/m/api/audio/a1.m4a#t=1");
    expect(calls).toEqual(["play"]);
  });
});
