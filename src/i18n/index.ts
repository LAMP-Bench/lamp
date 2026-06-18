import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import en from "./locales/en.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import de from "./locales/de.json";
import ru from "./locales/ru.json";
import pt from "./locales/pt.json";
import it from "./locales/it.json";
import zh from "./locales/zh.json";
import ja from "./locales/ja.json";
import ko from "./locales/ko.json";
import ar from "./locales/ar.json";
import hi from "./locales/hi.json";
import tr from "./locales/tr.json";

/// i18n bootstrap. Detected language order:
///  1. localStorage `lamp-bench-lang` if the user has explicitly picked one.
///  2. The OS locale the webview's `navigator` reports (this is what makes
///     a fresh install come up in the system language automatically).
///  3. Falls back to English when the OS locale isn't one we ship.
///
/// `load: "languageOnly"` + `nonExplicitSupportedLngs` normalise region
/// variants ("de-AT" → "de", "pt-BR" → "pt", "zh-CN" → "zh") so a Windows
/// set to German/Russian/etc. resolves to our base language. Locales that
/// only translate the common UI fall back per-key to English for the longer
/// technical strings.
i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .init({
    resources: {
      en: { translation: en },
      es: { translation: es },
      fr: { translation: fr },
      de: { translation: de },
      ru: { translation: ru },
      pt: { translation: pt },
      it: { translation: it },
      zh: { translation: zh },
      ja: { translation: ja },
      ko: { translation: ko },
      ar: { translation: ar },
      hi: { translation: hi },
      tr: { translation: tr },
    },
    fallbackLng: "en",
    supportedLngs: [
      "en", "es", "fr", "de", "ru", "pt", "it", "zh", "ja", "ko", "ar", "hi", "tr",
    ],
    load: "languageOnly",
    nonExplicitSupportedLngs: true,
    interpolation: { escapeValue: false },
    detection: {
      order: ["localStorage", "navigator"],
      lookupLocalStorage: "lamp-bench-lang",
      caches: ["localStorage"],
    },
  });

/// Keep the document language attribute in sync so screen readers, browser
/// spellcheck, and other a11y tooling pick up the change. Also flips the text
/// direction for RTL languages (Arabic) so the layout mirrors correctly.
const RTL = new Set(["ar", "he", "fa", "ur"]);
function syncHtmlLang(lng: string) {
  if (typeof document !== "undefined") {
    const short = lng.slice(0, 2);
    document.documentElement.lang = short;
    document.documentElement.dir = RTL.has(short) ? "rtl" : "ltr";
  }
}
syncHtmlLang(i18n.language);
i18n.on("languageChanged", syncHtmlLang);

/// Native-name labels for the language picker. Adding a language is one entry
/// here + a JSON under locales/ + an import above.
export const LANGUAGES = [
  { code: "en", label: "English" },
  { code: "es", label: "Español" },
  { code: "fr", label: "Français" },
  { code: "de", label: "Deutsch" },
  { code: "ru", label: "Русский" },
  { code: "pt", label: "Português" },
  { code: "it", label: "Italiano" },
  { code: "zh", label: "中文" },
  { code: "ja", label: "日本語" },
  { code: "ko", label: "한국어" },
  { code: "ar", label: "العربية" },
  { code: "hi", label: "हिन्दी" },
  { code: "tr", label: "Türkçe" },
] as const;

export default i18n;
