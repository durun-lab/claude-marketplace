import type { Topic } from "@mm/sdk";
import type { StoryWord } from "@mm/story/src/api.ts";
import { buildBlocks, formatClock, orphanTopics, shownText, toSeconds, type Block, type Span } from "@mm/story/src/model.ts";
import { RATING_LABELS } from "./model.ts";

export interface TranscriptProps {
  words: readonly StoryWord[];
  topics: readonly Topic[];
  span: Span | null;
  /** 範囲の 1 語目だけ押した状態。2 語目を待っている。 */
  waiting: boolean;
  /** 検索の結果で強調する単語。 */
  focusWord: string | null;
  onTapWord: (index: number) => void;
  /** 見出しを押すと、その話題の単語を全部選ぶ。 */
  onTapTopic: (topic: Topic) => void;
}

const inSpan = (span: Span | null, index: number): boolean => span !== null && index >= span.from && index <= span.to;

const TopicHead = ({ topic, onTap }: { topic: Topic; onTap: () => void }) => {
  const rating = topic.rating ?? "unrated";
  return (
    <button type="button" class="st-head" data-rating={rating} onClick={onTap}>
      <span class="st-title">{topic.title}</span>
      <span class="st-time">
        {formatClock(toSeconds(topic.in))} - {formatClock(toSeconds(topic.out))}
      </span>
      <span class="st-badge">{RATING_LABELS[rating]}</span>
    </button>
  );
};

const BlockView = ({ block, p }: { block: Block; p: TranscriptProps }) => (
  <section class="st-block" data-topic={block.topic !== null} data-rating={block.topic?.rating ?? "unrated"}>
    {block.topic && block.head && <TopicHead topic={block.topic} onTap={() => p.onTapTopic(block.topic as Topic)} />}
    {block.paragraphs.map((words) => (
      <p key={words[0]?.word.id} class="st-para">
        {words.map(({ word, index }) => (
          <button
            key={word.id}
            type="button"
            class="st-word"
            data-deleted={word.deleted}
            data-fixed={word.fixed !== null}
            data-selected={inSpan(p.span, index)}
            data-anchor={p.waiting && p.span?.from === index}
            data-focus={word.id === p.focusWord}
            aria-pressed={inSpan(p.span, index)}
            onClick={() => p.onTapWord(index)}
          >
            {shownText(word)}
          </button>
        ))}
      </p>
    ))}
  </section>
);

/** 文字起こし。話題ごとに区切り、使う（Keep）・使わない（Reject）・話題の外を色で分ける。 */
export const Transcript = (p: TranscriptProps) => {
  const blocks = buildBlocks(p.words, p.topics);
  const orphans = orphanTopics(blocks, p.topics);
  return (
    <div class="st-text">
      {blocks.map((block) => (
        <BlockView key={block.key} block={block} p={p} />
      ))}
      {orphans.length > 0 && (
        <section class="st-block" data-topic="true">
          <p class="st-meta">Topics without words</p>
          {orphans.map((t) => (
            <TopicHead key={t.id} topic={t} onTap={() => p.onTapTopic(t)} />
          ))}
        </section>
      )}
    </div>
  );
};
