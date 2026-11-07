import { useEffect, useState } from "preact/hooks";

/** `subscribe` と `get` を持つ部品の現在の値を、変わるたびに再描画へ渡す。 */
export const useStore = <T>(subscribe: (fn: () => void) => () => void, get: () => T): T => {
  const [value, setValue] = useState(get);
  useEffect(() => {
    setValue(get());
    return subscribe(() => setValue(get()));
  }, [subscribe, get]);
  return value;
};
