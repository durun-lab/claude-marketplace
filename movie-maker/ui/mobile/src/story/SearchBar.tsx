import { useState } from "preact/hooks";
import type { StoryHit } from "@mm/story/src/api.ts";
import { formatClock, toSeconds } from "@mm/story/src/model.ts";
import type { SearchState } from "./useStory.ts";

export interface SearchBarProps {
  search: SearchState;
  /** 結果を押したときの行き先。 */
  onOpen: (hit: StoryHit) => void;
}

/** 上の検索欄。送信で検索し、結果は 1 件ずつ押して該当の単語へ移る。 */
export const SearchBar = ({ search, onOpen }: SearchBarProps) => {
  const [text, setText] = useState("");
  const submit = (e: Event): void => {
    e.preventDefault();
    void search.run(text);
  };
  return (
    <div class="st-search">
      <form onSubmit={submit} role="search">
        <input
          type="search"
          enterkeyhint="search"
          aria-label="Search the transcripts"
          placeholder="Search words"
          value={text}
          onInput={(e) => {
            setText(e.currentTarget.value);
            if (e.currentTarget.value === "") search.clear();
          }}
        />
      </form>
      {search.hits && (
        <ul class="st-hits">
          {search.hits.length === 0 && <li class="st-meta">No matches.</li>}
          {search.hits.map((hit) => (
            <li key={`${hit.asset}-${hit.word}`}>
              <button type="button" onClick={() => onOpen(hit)}>
                <span class="st-time">{formatClock(toSeconds(hit.start))}</span>
                <span>{hit.context}</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};
