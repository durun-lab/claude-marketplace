import { useEffect, useState } from "preact/hooks";
import type { Topic } from "@mm/sdk";
import type { StoryOverview, StoryTranscript } from "@mm/story/src/api.ts";
import { compareTime, toSeconds, wordsIn } from "@mm/story/src/model.ts";
import type { StoryClient } from "./api.ts";
import { ArrangeSheet } from "./ArrangeSheet.tsx";
import { NO_PICK, spanOfTopic, tapWord, type Pick } from "./model.ts";
import { Footer, TopBar } from "./ReaderParts.tsx";
import { Transcript } from "./Transcript.tsx";
import { useActions } from "./useActions.ts";
import { usePlayer } from "./usePlayer.ts";

export interface ReaderProps {
  client: StoryClient;
  transcript: StoryTranscript;
  overview: StoryOverview | null;
  /** 検索の結果で開いたとき、選んで見せる単語。 */
  focusWord: string | null;
  onBack: () => void;
  /** 送った後に画面を読み直す。 */
  settled: () => Promise<void>;
}

/** 検索の結果の単語を 1 語の選択にする。 */
const usePickFromFocus = (transcript: StoryTranscript, focusWord: string | null, set: (p: Pick) => void): void => {
  useEffect(() => {
    const index = transcript.words.findIndex((w) => w.id === focusWord);
    if (index >= 0) set({ anchor: index, span: { from: index, to: index } });
  }, [focusWord, transcript.asset]);
};

/** 選んだ範囲の素材の音を鳴らす。鳴らしている間にもう一度押すと止める。 */
const usePlayRange = (client: StoryClient, transcript: StoryTranscript, pick: Pick) => {
  const player = usePlayer();
  const selected = wordsIn(transcript.words, pick.span);
  const play = (): void => {
    const first = selected[0];
    const last = selected.at(-1);
    if (player.playing || !first || !last) return player.stop();
    player.play(client.audioUrl(transcript.asset), toSeconds(first.start), toSeconds(last.end));
  };
  return { player, selected, play };
};

/** 画面が使う、素材の名前と並べ替えの候補（Reject していない話題を時刻順に）、並べる先のシーケンス。 */
const readerFacts = (transcript: StoryTranscript, overview: StoryOverview | null) => ({
  name: overview?.assets.find((a) => a.id === transcript.asset)?.name ?? transcript.asset,
  topics: [...transcript.topics].sort((a, b) => compareTime(a.in, b.in)).filter((t) => t.rating !== "rejected"),
  sequence: overview?.open_sequence ?? overview?.sequences[0]?.id ?? null,
});

/** 開いた素材の文字起こしと、選んだ範囲への操作。 */
export const Reader = ({ client, transcript, overview, focusWord, onBack, settled }: ReaderProps) => {
  const [pick, setPick] = useState<Pick>(NO_PICK);
  const [onPc, setOnPc] = useState(false);
  const [arranging, setArranging] = useState(false);
  const { player, selected, play } = usePlayRange(client, transcript, pick);
  const { name, topics, sequence } = readerFacts(transcript, overview);
  usePickFromFocus(transcript, focusWord, setPick);
  const actions = useActions({
    client,
    transcript,
    span: pick.span,
    sequence,
    onPc,
    settled,
    clearPick: () => setPick(NO_PICK),
  });
  const tapTopic = (topic: Topic): void => {
    const span = spanOfTopic(transcript.words, topic);
    if (span) setPick({ anchor: null, span });
  };
  return (
    <>
      <TopBar name={name} canArrange={topics.length > 0} onBack={onBack} onArrange={() => setArranging(true)} />
      <div class="st-scroll">
        <Transcript
          words={transcript.words}
          topics={transcript.topics}
          span={pick.span}
          waiting={pick.anchor !== null}
          focusWord={focusWord}
          onTapWord={(i) => setPick(tapWord(pick, i))}
          onTapTopic={tapTopic}
        />
      </div>
      {arranging && (
        <ArrangeSheet
          topics={topics}
          onClose={() => setArranging(false)}
          onInsert={(ordered) => {
            setArranging(false);
            void actions.arrange(ordered);
          }}
        />
      )}
      <Footer actions={actions} selected={selected} waiting={pick.anchor !== null} player={player} onPlay={play} onPc={{ on: onPc, set: setOnPc }} />
    </>
  );
};
