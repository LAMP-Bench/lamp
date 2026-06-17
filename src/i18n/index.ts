import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import en from "./locales/en.json";
import es from "./locales/es.json";

/// i18n bootstrap. Detected language order:
///  1. localStorage `lamp-bench-lang` if the user has explicitly picked one.
///  2. The OS language navigator reports.
///  3. Falls back to English.
/// The Settings → Language switcher writes back to localStorage so the
/// next launch boots straight into the chosen language.
i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
      es: { translation: es },
    },
    fallbackLng: "en",
    supportedLngs: ["en", "es", "fr"],
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "lamp-bench-lang",
      caches: ["localStorage"],
    },
  });

/// Keep the document language attribute in sync so screen readers, browser
/// spellcheck, and other a11y tooling pick up the change. i18next normalises
/// to short codes ("en", "es", "fr") — those are valid BCP-47 lang values.
function syncHtmlLang(lng: string) {
  if (typeof document !== "undefined") {
    document.documentElement.lang = lng.slice(0, 2);
  }
}
syncHtmlLang(i18n.language);
i18n.on("languageChanged", syncHtmlLang);

export const LANGUAGES = [
  { code: "en", label: "English" },
  { code: "es", label: "Español" },
  { code: "fr", label: "Français" },
] as const;

export default i18n;
