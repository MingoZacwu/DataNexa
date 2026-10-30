import { ArrowUpRight, BookOpen, Github } from "lucide-react";
import brandLogoUrl from "../../../resources/datanexa.png";
import type { I18nMessages } from "../../i18n";

const HELP_DOCS_URL = "https://mingozacwu.github.io/datanexa-site/help.html";
const ISSUES_URL = "https://github.com/MingoZacwu/DataNexa/issues/new/choose";

export function HelpView({
  t,
  onOpenHelp,
  onOpenIssues
}: {
  t: I18nMessages;
  onOpenHelp: () => void;
  onOpenIssues: () => void;
}) {
  return (
    <section className="help-page" aria-labelledby="help-title">
      <header className="help-masthead">
        <div className="help-masthead-copy">
          <h2 id="help-title" className="help-headline">
            <span className="help-headline-line">{t.help.headline}</span>
            <span className="help-headline-line help-headline-accent">
              {t.help.headlineAccent}
              <span className="help-headline-end">{t.help.headlineEnd}</span>
            </span>
          </h2>
          <p className="help-lead">{t.help.intro}</p>
        </div>
        <div className="help-masthead-visual" aria-hidden="true">
          <div className="help-halo">
            <span className="help-halo-glow" />
            <span className="help-halo-glow-2" />
            <span className="help-halo-ring" />
            <span className="help-halo-ring-2" />
            <span className="help-halo-disc"><img src={brandLogoUrl} alt="" /></span>
            <span className="help-halo-spark" />
            <span className="help-halo-spark-2" />
          </div>
        </div>
      </header>

      <div className="help-routes">
        <a
          className="help-route"
          href={HELP_DOCS_URL}
          onClick={(event) => {
            event.preventDefault();
            onOpenHelp();
          }}
        >
          <span className="help-route-top">
            <span className="help-route-icon"><BookOpen size={17} /></span>
            <strong className="help-route-title">{t.help.manualTitle}</strong>
          </span>
          <span className="help-route-text">{t.help.manualText}</span>
          <span className="help-route-action">
            {t.help.manualAction}
            <span className="help-route-arrow"><ArrowUpRight size={14} /></span>
          </span>
        </a>

        <a
          className="help-route"
          href={ISSUES_URL}
          onClick={(event) => {
            event.preventDefault();
            onOpenIssues();
          }}
        >
          <span className="help-route-top">
            <span className="help-route-icon"><Github size={17} /></span>
            <strong className="help-route-title">{t.help.issuesTitle}</strong>
          </span>
          <span className="help-route-text">{t.help.issuesText}</span>
          <span className="help-route-action">
            {t.help.issuesAction}
            <span className="help-route-arrow"><ArrowUpRight size={14} /></span>
          </span>
        </a>
      </div>
    </section>
  );
}
