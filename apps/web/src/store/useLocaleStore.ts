import { create } from "zustand";

export type Locale = "en" | "hi";

type TranslationKey =
  | "header_tagline"
  | "signin"
  | "logout"
  | "my_uploads"
  | "alerts"
  | "search_placeholder"
  | "search_error"
  | "search_no_results";

const STORAGE_KEY = "ttl_locale";

const TRANSLATIONS: Record<Locale, Record<TranslationKey, string>> = {
  en: {
    header_tagline:
      "Explore, browse through, learn, and contribute your collected or photographed street letterings and typefaces.",
    signin: "Sign In",
    logout: "Logout",
    my_uploads: "My Uploads",
    alerts: "Alerts",
    search_placeholder: "Search tags, scripts, neighborhoods...",
    search_error: "Search failed. Try again.",
    search_no_results: "No matching entries found.",
  },
  hi: {
    header_tagline:
      "स्ट्रीट लेटरिंग और टाइपफ़ेस खोजें, देखें, सीखें और अपना संग्रह भी जोड़ें।",
    signin: "साइन इन",
    logout: "लॉगआउट",
    my_uploads: "मेरे अपलोड",
    alerts: "सूचनाएं",
    search_placeholder: "टैग, लिपि, इलाके खोजें...",
    search_error: "खोज विफल रही। फिर से प्रयास करें।",
    search_no_results: "कोई मिलती-जुलती प्रविष्टि नहीं मिली।",
  },
};

function getInitialLocale(): Locale {
  if (typeof window === "undefined") return "en";
  const stored = window.localStorage.getItem(STORAGE_KEY);
  return stored === "hi" ? "hi" : "en";
}

interface LocaleState {
  locale: Locale;
  setLocale: (locale: Locale) => void;
  t: (key: TranslationKey) => string;
}

export const useLocaleStore = create<LocaleState>((set, get) => ({
  locale: getInitialLocale(),
  setLocale: (locale) => {
    if (typeof window !== "undefined") {
      window.localStorage.setItem(STORAGE_KEY, locale);
    }
    set({ locale });
  },
  t: (key) => {
    const locale = get().locale;
    return TRANSLATIONS[locale][key] ?? TRANSLATIONS.en[key] ?? key;
  },
}));
