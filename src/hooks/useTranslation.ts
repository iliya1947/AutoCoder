import { useState, useEffect } from "react";
import ru from "../locales/ru.json";
import en from "../locales/en.json";
import he from "../locales/he.json";

export type Language = "ru" | "en" | "he";

const translations: Record<Language, Record<string, string>> = {
  ru,
  en,
  he,
};

export function useTranslation() {
  const [lang, setLang] = useState<Language>(() => {
    const saved = localStorage.getItem("autocoder_lang");
    return (saved as Language) || "ru";
  });

  const t = (key: string): string => {
    return translations[lang]?.[key] || key;
  };

  const changeLanguage = (newLang: Language) => {
    setLang(newLang);
    localStorage.setItem("autocoder_lang", newLang);
    if (newLang === "he") {
      document.documentElement.dir = "rtl";
    } else {
      document.documentElement.dir = "ltr";
    }
  };

  useEffect(() => {
    if (lang === "he") {
      document.documentElement.dir = "rtl";
    } else {
      document.documentElement.dir = "ltr";
    }
  }, [lang]);

  return { t, lang, changeLanguage };
}
