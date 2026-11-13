import * as Switch from "@radix-ui/react-switch";
import clsx from "clsx";
import { formatMessage, type I18nMessages, type Locale } from "../../i18n";
import { isWriteTool, writeMessages, writeToolName } from "../../write-i18n";
import type { McpToolInfo } from "../../types";
import { toolDisplayName, toolIcon, toolIntro } from "../../app/utils";

export function ToolsView({ t, locale, tools, busy, onToggle }: {
  t: I18nMessages;
  locale: Locale;
  tools: McpToolInfo[];
  busy: boolean;
  onToggle: (name: string, enabled: boolean) => void;
}) {
  const wt = writeMessages[locale];
  // Tools live on this page, including the restricted-write tools: a write tool
  // is switched on and off exactly like any other MCP tool.
  const groups = [
    { key: "discovery", title: t.tools.groups.discovery, tools: tools.filter((tool) => ["datanexa_list_connections", "datanexa_get_schema", "datanexa_describe_table"].includes(tool.name)) },
    { key: "access", title: t.tools.groups.access, tools: tools.filter((tool) => ["datanexa_sample_rows", "datanexa_execute_readonly_sql"].includes(tool.name)) },
    { key: "analysis", title: t.tools.groups.analysis, tools: tools.filter((tool) => ["datanexa_explain_sql", "datanexa_policy_check"].includes(tool.name)) },
    { key: "write", title: wt.title, tools: tools.filter((tool) => isWriteTool(tool.name)) }
  ];

  return (
    <section className="tools-page">
      <div className="panel tools-summary"><h2>{formatMessage(t.tools.summary, { enabled: tools.filter((tool) => tool.enabled).length, total: tools.length })}</h2></div>
      <div className="tools-list">
        {groups.map((group) => (
          <section className="tool-group" key={group.key}>
            <header><span>{group.title}</span><small>{group.tools.filter((tool) => tool.enabled).length} / {group.tools.length}</small></header>
            {group.tools.map((tool) => {
              const ToolIcon = toolIcon(tool.name);
              const name = writeToolName(locale, tool.name) ?? toolDisplayName(t, tool.name);
              return (
                <article className={clsx("tool-card", !tool.enabled && "disabled")} key={tool.name}>
                  <span className="tool-icon"><ToolIcon size={16} aria-hidden="true" /></span>
                  <div className="tool-body">
                    <div className="tool-title-row"><div><strong>{name}</strong><code>{tool.name}</code></div></div>
                    <p>{toolIntro(t, tool)}</p>
                  </div>
                  <Switch.Root className="switch" checked={tool.enabled} disabled={busy} onCheckedChange={(checked) => onToggle(tool.name, checked)} aria-label={formatMessage(t.tools.toggle, { name: tool.name })}><Switch.Thumb className="switch-thumb" /></Switch.Root>
                </article>
              );
            })}
          </section>
        ))}
      </div>
    </section>
  );
}
