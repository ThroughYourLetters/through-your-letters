import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { HelmetProvider } from "react-helmet-async";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { Analytics } from "@vercel/analytics/react";
import { webLimits } from "./lib/runtimeConfig";
import App from "./App.tsx";
import "./index.css";
import { useAuthStore } from "./store/useAuthStore";

const createQueryClient = () =>
  new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 1000 * webLimits.query_stale_seconds,
        retry: webLimits.query_retries,
      },
    },
  });

function SessionApp() {
  const revision = useAuthStore((state) => state.revision);
  const client = React.useMemo(createQueryClient, [revision]);
  React.useEffect(() => () => client.clear(), [client]);
  return (
    <QueryClientProvider key={revision} client={client}>
      <App />
    </QueryClientProvider>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <HelmetProvider>
      <BrowserRouter>
        <SessionApp />
      </BrowserRouter>
    </HelmetProvider>
    <Analytics />
  </React.StrictMode>,
);
