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
        <div className="text-[120px] md:text-[200px] font-semibold leading-none tracking-tight text-ink/5">
          404
        </div>
        <h1 className="font-display font-semibold text-4xl tracking-tight -mt-20">
          Page Not Found
        </h1>
        <p className="text-sm font-medium text-stone-500 max-w-md">
          This street doesn't exist in our archive yet. Maybe it's waiting to be discovered.
        </p>
        <Link
          to="/"
          className="btn-dark !py-4"
        >
          <Compass size={16} />
          Back to Gallery
        </Link>
      </div>
    </>
  );
};

export default NotFoundPage;
