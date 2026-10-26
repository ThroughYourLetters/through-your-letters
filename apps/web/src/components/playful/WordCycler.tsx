import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { usePrefersReducedMotion } from "./usePrefersReducedMotion";

// What a street sign quietly carries, shown one after another
const WORDS = [
  "a story",
  "a memory",
  "a feeling",
  "a secret",
  "a little joke",
  "a love letter",
  "a lifetime",
];

const INTERVAL_MS = 2200;

const WordCycler: React.FC<{ className?: string }> = ({ className = "" }) => {
  const reduced = usePrefersReducedMotion();
  const [index, setIndex] = useState(0);
  const [paused, setPaused] = useState(false);

  useEffect(() => {
    if (reduced || paused) return;
    const timer = window.setInterval(
      () => setIndex((i) => (i + 1) % WORDS.length),
      INTERVAL_MS,
    );
    return () => window.clearInterval(timer);
  }, [reduced, paused]);

  const word = WORDS[index];
  const wordRefs = useRef<Array<HTMLSpanElement | null>>([]);
  const [width, setWidth] = useState<number>();

  // Size the slot to the visible word so the full stop follows it closely
  useLayoutEffect(() => {
    const measure = () => setWidth(wordRefs.current[index]?.offsetWidth);
    measure();
    window.addEventListener("resize", measure);
    document.fonts?.ready.then(measure).catch(() => {});
    return () => window.removeEventListener("resize", measure);
  }, [index]);

  return (
    <button
      type="button"
      onClick={() => setIndex((i) => (i + 1) % WORDS.length)}
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      aria-label={`${word}. Click for another word.`}
      title="Click for another word"
      style={width ? { width } : undefined}
      className={`group relative inline-grid items-baseline justify-items-start cursor-pointer align-baseline text-left leading-[inherit] transition-[width] duration-500 ease-out ${className}`}
    >
      {/* Words share one grid cell; the slot width animates between them */}
      {WORDS.map((w, i) => (
        <span
          key={w}
          ref={(el) => {
            wordRefs.current[i] = el;
          }}
          aria-hidden={i !== index}
          style={{ gridArea: "1 / 1" }}
          className={`w-max whitespace-nowrap pr-[0.12em] leading-[1.02] transition-all duration-500 ease-out ${
            i === index
              ? "opacity-100 translate-y-0 blur-0"
              : "pointer-events-none opacity-0 translate-y-3 blur-sm"
          }`}
        >
          {w}
        </span>
      ))}
      <span className="pointer-events-none absolute -bottom-1 left-0 translate-y-full rounded-full bg-ink px-2 py-0.5 font-sans text-[11px] font-medium not-italic tracking-normal text-paper opacity-0 transition-opacity group-hover:opacity-100 group-focus-visible:opacity-100">
        click for another
      </span>
    </button>
  );
};

export default WordCycler;
