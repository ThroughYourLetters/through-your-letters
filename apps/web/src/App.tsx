import React, { Suspense, lazy, useEffect } from "react";
import {
  Routes,
  Route,
  useLocation,
  NavLink,
  useNavigate,
} from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { syncOfflineUploads } from "./lib/offlineQueue";
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
      const synced = await syncOfflineUploads();
      if (synced > 0) {
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
    <div className="min-h-screen flex flex-col max-w-6xl mx-auto bg-white/40 shadow-2xl relative border-x-4 border-black zine-texture">
      <div className="grain-overlay"></div>
      <Header />
      <ToastContainer />
      <ScrollToTop />

      <main className="flex-1 overflow-y-auto px-6 md:px-16 py-16 relative">
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
              <Route path="/collections/:id" element={<CollectionDetailPage />} />
              <Route path="*" element={<NotFoundPage />} />
            </Routes>
          </Suspense>
        </ErrorBoundary>
      </main>

      <BottomNav />
    </div>
  );
};

function BottomNav() {
  const navItems = [
    { to: "/", icon: Compass, label: "Explore", end: true },
    { to: "/contribute", icon: PlusCircle, label: "Contribute" },
    { to: "/map", icon: MapIcon, label: "Map" },
    { to: "/community", icon: Trophy, label: "Community" },
    { to: "/about", icon: Info, label: "Info" },
  ];

  return (
    <nav className="sticky bottom-10 self-center w-[92%] md:w-[65%] bg-white border-4 border-black p-6 flex justify-between items-center z-50 brutalist-shadow-lg mx-auto mb-10 transition-all hover:scale-[1.01]">
      {navItems.map(({ to, icon: Icon, label, end }) => (
        <NavLink
          key={to}
          to={to}
          end={end}
          className={({ isActive }) =>
            `flex-1 flex flex-col items-center gap-1.5 font-black text-[11px] uppercase ${
              isActive ? "text-[#cc543a]" : "text-slate-400"
            }`
          }
        >
          <Icon size={28} />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}

function RouteLoading() {
  return (
    <div className="py-24 text-center">
      <p className="text-[11px] font-black uppercase tracking-widest text-slate-500">
        Loading view...
      </p>
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
        await queryClient.invalidateQueries({
          queryKey: ["letterings-infinite"],
        });
        navigate("/");
      }}
    />
  );
}

export default App;
