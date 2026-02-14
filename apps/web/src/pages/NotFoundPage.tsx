import React from "react";
import { Link } from "react-router-dom";
import { Helmet } from "react-helmet-async";
import { Compass } from "lucide-react";

const NotFoundPage: React.FC = () => {
  return (
    <>
      <Helmet>
        <title>404 | Through Your Letters</title>
      </Helmet>
      <div className="min-h-[60vh] flex flex-col items-center justify-center text-center space-y-8 py-20">
        <div className="text-[120px] md:text-[200px] font-black leading-none tracking-tighter text-black/5">
          404
        </div>
        <h1 className="text-4xl font-black uppercase tracking-tighter -mt-20">
          Page Not Found
        </h1>
        <p className="text-sm font-medium text-slate-500 max-w-md">
          This street doesn't exist in our archive yet. Maybe it's waiting to be discovered.
        </p>
        <Link
          to="/"
          className="inline-flex items-center gap-2 bg-black text-white px-8 py-4 text-[10px] font-black uppercase tracking-widest hover:bg-[#cc543a] transition-colors brutalist-shadow-sm"
        >
          <Compass size={16} />
          Back to Gallery
        </Link>
      </div>
    </>
  );
};

export default NotFoundPage;
