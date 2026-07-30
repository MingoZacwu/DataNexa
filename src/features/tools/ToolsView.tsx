import * as Switch from "@radix-ui/react-switch";
import clsx from "clsx";
import { Database, ShieldCheck, Wrench } from "lucide-react";
import { formatMessage, type I18nMessages } from "../../i18n";
import type { McpToolInfo } from "../../types";
import { toolDisplayName, toolIntro } from "../../app/utils";

export function ToolsView({
  t,
  tools,
  busy,
  onToggle
}: {
  t: I18nMessages;
  tools: McpToolInfo[];
  busy: boolean;
  onToggle: (name: string, enabled: boolean) => void;
}) {
  const enabledCount = tools.filter((tool) => tool.enabled).length;
  const groups = [
    { key: "discovery" as const, tools: tools.filter((tool) => ["datanexa_list_connections", "datanexa_get_schema", "datanexa_describe_table"].includes(tool.name)) },
    { key: "access" as const, tools: tools.filter((tool) => ["datanexa_sample_rows", "datanexa_execute_readonly_sql"].includes(tool.name)) },
    { key: "analysis" as const, tools: tools.filter((tool) => ["datanexa_explain_sql", "datanexa_policy_check"].includes(tool.name)) }
  ];

  return (
    <section className="tools-page">
      <div className="panel tools-summary">
        <div>
          <h2>{formatMessage(t.tools.summary, { enabled: enabledCount, total: tools.length })}</h2>
        </div>
      </div>

      <div className="tools-list">
        {groups.map((group) => (
          <section className="tool-group" key={group.key}>
            <header><span>{t.tools.groups[group.key]}</span><small>{group.tools.filter((tool) => tool.enabled).length} / {group.tools.length}</small></header>
            {group.tools.map((tool) => (
              <article className={clsx("tool-card", !tool.enabled && "disabled")} key={tool.name}>
                <span className="tool-signal" />
                <div className="tool-body">
                  <div className="tool-title-row">
                    <div>
                      <strong>{toolDisplayName(t, tool.name)}</strong>
                      <code>{tool.name}</code>
                    </div>
                  </div>
                  <p>{toolIntro(t, tool)}</p>
                </div>
                <Switch.Root className="switch" checked={tool.enabled} disabled={busy} onCheckedChange={(checked) => onToggle(tool.name, checked)} aria-label={formatMessage(t.tools.toggle, { name: tool.name })}>
                  <Switch.Thumb className="switch-thumb" />
                </Switch.Root>
              </article>
            ))}
          </section>
        ))}
      </div>
    </section>
  );
}

