import React, { useEffect, useRef, useState } from "react";
import { usePrefersReducedMotion } from "./usePrefersReducedMotion";

// The "T" of the mark flips through scripts on hover, and once in a while on its own
const GLYPHS = [
  { t: "Ty", font: "" },
  { t: "ತ", font: '"Noto Sans Kannada"' },
  { t: "त", font: '"Noto Sans Devanagari"' },
  { t: "த", font: '"Noto Sans Tamil"' },
  { t: "ত", font: '"Noto Sans Bengali"' },
  { t: "ത", font: '"Noto Sans Malayalam"' },
];

const LogoMark: React.FC = () => {
  const reduced = usePrefersReducedMotion();
  const [index, setIndex] = useState(0);
  const runningRef = useRef<number | null>(null);

  const play = () => {
    if (reduced || runningRef.current !== null) return;
    let step = 0;
    runningRef.current = window.setInterval(() => {
      step += 1;
      if (step >= GLYPHS.length) {
        window.clearInterval(runningRef.current!);
        runningRef.current = null;
        setIndex(0);
        return;
      }
      setIndex(step);
    }, 130);
  };

  const playRef = useRef(play);
  playRef.current = play;

  useEffect(() => {
    if (reduced) return;
    const idle = window.setInterval(() => playRef.current(), 9000);
    return () => {
      window.clearInterval(idle);
      if (runningRef.current !== null) window.clearInterval(runningRef.current);
      runningRef.current = null;
    };
  }, [reduced]);

  const glyph = GLYPHS[index];

  return (
    <span
      onMouseEnter={play}
      className="flex h-9 w-9 items-center justify-center rounded-xl bg-ink text-lg text-paper transition-transform duration-300 hover:-rotate-6 hover:scale-110"
      aria-hidden="true"
    >
      <span
        key={index}
        className={index === 0 ? "font-serif italic" : "animate-[pop_0.18s_ease-out]"}
        style={glyph.font ? { fontFamily: glyph.font } : undefined}
      >
        {glyph.t}
      </span>
    </span>
  );
};

export default LogoMark;
