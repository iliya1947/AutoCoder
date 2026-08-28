import { Language, useTranslation } from "../hooks/useTranslation";

const languageNames: Record<Language, string> = { ru: "Русский", en: "English", he: "עברית" };

export function WorkspaceHeader({ onOpenBackups, backupsDisabled }: { onOpenBackups: () => void; backupsDisabled: boolean }) {
  const { t, lang, changeLanguage } = useTranslation();
  return <header className="app-header">
    <div><h1>{t("app.title")}</h1><p>{t("app.tagline")}</p></div>
    <div className="header-actions"><button className="secondary-button header-button" disabled={backupsDisabled} onClick={onOpenBackups}>{t("backups.open")}</button><label className="language-picker">
      <span>{t("settings.language")}</span>
      <select aria-label={t("settings.language")} value={lang} onChange={(event) => changeLanguage(event.target.value as Language)}>
        {Object.entries(languageNames).map(([language, name]) => <option key={language} value={language}>{name}</option>)}
      </select>
    </label></div>
  </header>;
}
