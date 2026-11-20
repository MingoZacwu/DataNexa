import type { JdbcDriverBundle } from "../../types";

export interface JdbcUrlTemplate {
  id: string;
  label: string;
  url: string;
}

interface TemplateRule extends JdbcUrlTemplate {
  /** Maven artifactIds (or prefixes) that identify the vendor. */
  artifacts: string[];
  /** Driver class prefixes that identify the vendor, used for local imports. */
  classPrefixes: string[];
}

/**
 * Starting points for the JDBC URL field. Each driver only knows which URLs it
 * accepts (`Driver.acceptsURL`), it cannot report a template, so the vendor is
 * inferred from the bundle's Maven coordinate or its driver classes.
 */
const RULES: TemplateRule[] = [
  {
    id: "mysql",
    label: "MySQL",
    url: "jdbc:mysql://localhost:3306/database",
    artifacts: ["mysql-connector-j"],
    classPrefixes: ["com.mysql."]
  },
  {
    id: "mariadb",
    label: "MariaDB",
    url: "jdbc:mariadb://localhost:3306/database",
    artifacts: ["mariadb-java-client"],
    classPrefixes: ["org.mariadb.jdbc."]
  },
  {
    id: "postgres",
    label: "PostgreSQL",
    url: "jdbc:postgresql://localhost:5432/database",
    artifacts: ["postgresql"],
    classPrefixes: ["org.postgresql."]
  },
  {
    id: "sqlserver",
    label: "SQL Server",
    url: "jdbc:sqlserver://localhost:1433;databaseName=database;encrypt=true;trustServerCertificate=true",
    artifacts: ["mssql-jdbc"],
    classPrefixes: ["com.microsoft.sqlserver."]
  },
  {
    id: "oracle",
    label: "Oracle",
    url: "jdbc:oracle:thin:@//localhost:1521/ORCLPDB1",
    artifacts: ["ojdbc"],
    classPrefixes: ["oracle.jdbc."]
  },
  {
    id: "db2",
    label: "IBM Db2",
    url: "jdbc:db2://localhost:50000/database",
    artifacts: ["jcc", "db2jcc"],
    classPrefixes: ["com.ibm.db2."]
  },
  {
    id: "clickhouse",
    label: "ClickHouse",
    url: "jdbc:clickhouse://localhost:8123/database",
    artifacts: ["clickhouse-jdbc"],
    classPrefixes: ["com.clickhouse."]
  },
  {
    id: "snowflake",
    label: "Snowflake",
    url: "jdbc:snowflake://account.snowflakecomputing.com/?db=DATABASE&warehouse=WAREHOUSE",
    artifacts: ["snowflake-jdbc"],
    classPrefixes: ["net.snowflake."]
  }
];

function bundleArtifact(bundle: JdbcDriverBundle) {
  const parts = bundle.maven_coordinate.split(":");
  return parts.length >= 2 ? parts[1].trim().toLowerCase() : "";
}

export function matchJdbcTemplate(bundle: JdbcDriverBundle | null | undefined): JdbcUrlTemplate | null {
  if (!bundle) return null;
  const artifact = bundleArtifact(bundle);
  const classes = bundle.driver_classes ?? [];

  for (const rule of RULES) {
    if (classes.some((name) => rule.classPrefixes.some((prefix) => name.startsWith(prefix)))) {
      return { id: rule.id, label: rule.label, url: rule.url };
    }
    if (artifact && rule.artifacts.some((candidate) => artifact.startsWith(candidate))) {
      return { id: rule.id, label: rule.label, url: rule.url };
    }
  }
  return null;
}

function isTemplateUrl(url: string) {
  const normalized = url.trim().toLowerCase();
  return RULES.some((rule) => rule.url.toLowerCase() === normalized);
}

/**
 * Template for the selected bundle. An empty field is filled outright; a field
 * still holding another template follows the bundle switch, while anything the
 * user typed is left untouched.
 */
export function suggestJdbcUrl(current: string | null | undefined, bundle: JdbcDriverBundle | null | undefined) {
  const value = current ?? "";
  const template = matchJdbcTemplate(bundle);
  if (!template) return value;
  if (!value.trim() || isTemplateUrl(value)) return template.url;
  return value;
}
