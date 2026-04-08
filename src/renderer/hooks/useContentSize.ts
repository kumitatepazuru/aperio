import { useCallback, useState, type RefCallback } from "react";

const useContentSize = () => {
  const [size, setSize] = useState({ width: 0, height: 0 });
  const ref = useCallback<RefCallback<HTMLElement>>((elem) => {
    if (!elem) return;

    const observer = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      setSize({ width, height });
    });

    observer.observe(elem);

    return () => {
      observer.disconnect();
    };
  }, []);

  return { size, ref };
};

export default useContentSize;
