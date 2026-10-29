import { invalidatePublicLetterings } from "./lib/publicQueries";
import React, { Suspense, lazy, useEffect } from "react";
import { createPortal } from "react-dom";
import {
  Routes,
  Route,
  useLocation,
  NavLink,
  useNavigate,
} from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { syncOfflineUploads } from "./lib/offlineQueue";
import { captureUserSession, isCurrentUserSession } from "./lib/userSession";
import Header from "./components/Header";
import ToastContainer from "./components/ui/ToastContainer";
import ErrorBoundary from "./components/ErrorBoundary";
import { useToastStore } from "./store/useToastStore";
import { useAuthStore } from "./store/useAuthStore";
import {
  Compass,
  PlusCircle,
  Map as MapIcon,
  Info,
  Trophy,
} from "lucide-react";

const ExplorePage = lazy(() => import("./pages/ExplorePage"));
const AboutPage = lazy(() => import("./pages/AboutPage"));
const NotFoundPage = lazy(() => import("./pages/NotFoundPage"));
const LetteringDetailPage = lazy(() => import("./pages/LetteringDetailPage"));
const AuthPage = lazy(() => import("./pages/AuthPage"));
const MyUploadsPage = lazy(() => import("./pages/MyUploadsPage"));
const NotificationsPage = lazy(() => import("./pages/NotificationsPage"));
const ContributionPanel = lazy(() => import("./components/ContributionPanel"));
const MapSection = lazy(() => import("./components/MapSection"));
const AdminPanel = lazy(() => import("./components/AdminPanel"));
const ContributorProfile = lazy(
  () => import("./components/ContributorProfile"),
);
const CommunityPage = lazy(() => import("./components/CommunityPage"));
const CollectionDetailPage = lazy(() => import("./pages/CollectionDetailPage"));

// Scroll to top on route change
function ScrollToTop() {
  const { pathname } = useLocation();
  useEffect(() => {
    window.scrollTo(0, 0);
  }, [pathname]);
  return null;
}

const App: React.FC = () => {
  const { addToast } = useToastStore();
  const { hydrated, hydrate } = useAuthStore();
  const navigate = useNavigate();
  const location = useLocation();

  // Sync offline uploads when connectivity returns
  useEffect(() => {
    const handleOnline = async () => {
      const session = captureUserSession();
      const synced = await syncOfflineUploads();
      if (synced > 0 && isCurrentUserSession(session)) {
        addToast(
          `${synced} offline upload${synced > 1 ? "s" : ""} synced`,
          "success",
        );
      }
    };
    window.addEventListener("online", handleOnline);
    return () => window.removeEventListener("online", handleOnline);
  }, [addToast]);

  useEffect(() => {
    if (!hydrated) hydrate();
  }, [hydrated, hydrate]);

  useEffect(() => {
    const params = new URLSearchParams(location.search);
    if (params.has("admin")) {
      navigate("/admin", { replace: true });
    }
  }, [location.search, navigate]);

  return (
    <div className="min-h-screen min-h-dvh flex flex-col">
      <Header />
      <ToastContainer />
      <ScrollToTop />

      <main className="flex-1 page-container pt-8 md:pt-12 pb-28 md:pb-20 relative">
        <ErrorBoundary>
          <Suspense fallback={<RouteLoading />}>
            <Routes>
              <Route path="/" element={<ExplorePage />} />
              <Route path="/contribute" element={<ContributeRoute />} />
              <Route path="/map" element={<MapSection />} />
              <Route path="/community" element={<CommunityPage />} />
              <Route path="/about" element={<AboutPage />} />
              <Route path="/auth" element={<AuthPage />} />
              <Route path="/me/uploads" element={<MyUploadsPage />} />
              <Route path="/me/notifications" element={<NotificationsPage />} />
              <Route
                path="/admin"
                element={<AdminPanel onClose={() => window.history.back()} />}
              />
              <Route
                path="/contributor/:tag"
                element={
                  <ContributorProfile onBack={() => window.history.back()} />
                }
              />
              <Route path="/lettering/:id" element={<LetteringDetailPage />} />
              <Route
                path="/collections/:id"
                element={<CollectionDetailPage />}
              />
              <Route path="*" element={<NotFoundPage />} />
            </Routes>
          </Suspense>
        </ErrorBoundary>
      </main>

      <Footer />
      <BottomNav />
    </div>
  );
};

function BottomNav() {
  const navItems = [
    { to: "/", icon: Compass, label: "Explore", end: true },
    { to: "/map", icon: MapIcon, label: "Map" },
    { to: "/contribute", icon: PlusCircle, label: "Add", primary: true },
    { to: "/community", icon: Trophy, label: "Community" },
    { to: "/about", icon: Info, label: "Info" },
  ];

  return createPortal(
    <nav
      aria-label="Quick navigation"
      className="mobile-bottom-nav md:hidden fixed inset-x-3 z-50 flex items-center justify-between rounded-2xl border border-line bg-surface/95 px-2 py-1.5 shadow-lift backdrop-blur"
    >
      {navItems.map(({ to, icon: Icon, label, end, primary }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          className={({ isActive }) =>
            primary
              ? "flex h-11 w-11 items-center justify-center rounded-full bg-terracotta text-white shadow-card"
              : `flex flex-1 flex-col items-center gap-0.5 py-1 text-[10px] font-medium ${
                  isActive ? "text-terracotta" : "text-muted"
                }`
          }
          aria-label={label}
        >
          <Icon size={primary ? 22 : 20} strokeWidth={1.75} />
          {!primary && label}
        </NavLink>
      ))}
    </nav>,
    document.body,
  );
}

function Footer() {
  return (
    <footer className="site-footer border-t border-line mt-auto">
      <div className="page-container flex flex-col gap-6 py-10 md:flex-row md:items-end md:justify-between">
        <div className="space-y-2">
          <p className="font-display text-2xl tracking-tight">
            Through Your <em className="text-terracotta">Letters</em>
          </p>
          <p className="text-sm text-muted max-w-md">
            A collaborative archive of street typography and lettering, starting
            in Bengaluru.
          </p>
        </div>
        <div className="text-sm md:text-right">
          <p className="eyebrow">Initiated and designed by</p>
          <p className="font-medium mt-1">Akankshya Pradhan</p>
          <p className="eyebrow mt-4">Developed by</p>
          <p className="font-medium mt-1">Vikalp Mishra</p>
        </div>
      </div>
    </footer>
  );
}

function RouteLoading() {
  return (
    <div className="py-24 flex justify-center">
      <span className="eyebrow animate-pulse">Loading…</span>
    </div>
  );
}

function ContributeRoute() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  return (
    <ContributionPanel
      onCancel={() => navigate(-1)}
      onSubmit={async () => {
        await invalidatePublicLetterings(queryClient);
        navigate("/");
      }}
    />
  );
}

export default App;
