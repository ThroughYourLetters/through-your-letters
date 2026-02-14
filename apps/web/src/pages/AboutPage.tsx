import React, { useState } from "react";
import { Helmet } from "react-helmet-async";
import { Puzzle } from "lucide-react";

const SCRIPT_SPECIMENS = [
  { char: "\u0B05", lang: "Odia", font: "odia", color: "bg-[#cc543a] text-white" },
  { char: "\u0C95", lang: "Kannada", font: "kannada", color: "bg-black text-white" },
  { char: "\u0C85", lang: "Kannada", font: "kannada", color: "bg-slate-200 text-black" },
  { char: "\u0905", lang: "Hindi", font: "devanagari", color: "bg-[#cc543a] text-white" },
  { char: "\u0939", lang: "Marathi", font: "devanagari", color: "bg-black text-white" },
  { char: "\u0D05", lang: "Malayalam", font: "malayalam", color: "bg-[#2d5a27] text-white" },
  { char: "\u0627", lang: "Urdu", font: "urdu", color: "bg-[#d4a017] text-white" },
  { char: "\uABC0", lang: "Manipuri", font: "latin", color: "bg-slate-800 text-white" },
  { char: "A", lang: "Latin", font: "", color: "bg-slate-100 text-black" },
];

const ScriptPuzzleGrid = () => {
  const [activeItem, setActiveItem] = useState<number | null>(null);
  return (
    <div className="pixel-grid">
      {SCRIPT_SPECIMENS.map((item, idx) => (
        <button
          key={idx}
          onClick={() => setActiveItem(idx === activeItem ? null : idx)}
          className={`${item.color} aspect-square flex items-center justify-center border border-black/10 relative overflow-hidden group`}
        >
          <span className={`text-2xl font-black ${item.font} transition-transform group-hover:scale-125`}>
            {item.char}
          </span>
          {activeItem === idx && (
            <div className="absolute inset-0 bg-black/90 flex flex-col items-center justify-center p-1">
              <span className="text-[7px] font-black uppercase text-white mb-1">{item.lang}</span>
              <div className="w-4 h-[1px] bg-[#cc543a]"></div>
            </div>
          )}
        </button>
      ))}
    </div>
  );
};

const AboutPage: React.FC = () => {
  return (
    <>
      <Helmet>
        <title>About | Through Your Letters</title>
      </Helmet>
      <div className="max-w-4xl mx-auto py-20 space-y-32">
        <h2 className="text-7xl md:text-9xl font-black uppercase italic leading-[0.7]">
          A Personal <span className="text-[#cc543a]">Note.</span>
        </h2>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-20">
          <div className="space-y-8">
            <p className="handwritten text-2xl leading-relaxed text-slate-900 font-bold border-l-4 border-black pl-8">
              Hello! This project started as a personal curiosity for street
              lettering. When I was a child, I spent my time reading
              magazines, books, and charts that my father collected
              passionately. Every evening, when we went out for ice cream,
              we would look at the signboards on shops and streets. He used
              to tell me how to read and pronounce them, and eventually I
              realized how much I love the way letters are created, styled,
              painted and so on.
            </p>
            <p className="serif text-xl leading-relaxed text-slate-700 italic">
              My mother used to show me those same charts to get me to eat
              my food, so I believe that's where my fascination with
              letterforms truly began—haha, call it storytelling.
            </p>
            <p className="serif text-xl leading-relaxed text-slate-800">
              Throughout my time in academia, I've been collecting, reading,
              and even presenting projects on this niche interest. Now, I
              feel I finally have something to truly get started with. I've
              always loved capturing lettering and investigating the stories
              hidden behind them.
            </p>
          </div>
          <div className="space-y-12 relative z-10">
            <div className="bg-black text-white p-14 brutalist-shadow-lg transform rotate-1">
              <p className="text-xl leading-snug font-bold mb-8 italic">
                There is no better place to start than Bengaluru, where I
                aim to build an open-source platform by the people, for the
                people, for street lettering archival.
              </p>
              <p className="text-base opacity-80 leading-relaxed font-medium">
                The intent is to create an archive, give credit, learn,
                share stories, and remember our histories. I am putting
                something I genuinely care about and have fun doing here for
                you.
              </p>
            </div>
            <p className="handwritten text-2xl leading-relaxed text-slate-900 font-bold border-l-8 border-[#cc543a] pl-8 py-6">
              This is my attempt to give a home to my collected letterings
              and, if you want, yours too. Go have fun with this! Upload
              your letters, describe them, or just add a fun anecdote. You
              can also see what others have created. And last but not
              least—thank you.
            </p>
          </div>
        </div>
        <div className="pt-32 border-t-8 border-black">
          <h3 className="text-5xl font-black uppercase mb-12 flex items-center gap-4">
            <Puzzle size={40} /> Letters and Bits
          </h3>
          <ScriptPuzzleGrid />
        </div>
      </div>
    </>
  );
};

export default AboutPage;
