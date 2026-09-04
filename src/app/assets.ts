import mysqlLogoUrl from "../../resources/db-logo/mysql.png";
import postgresLogoUrl from "../../resources/db-logo/postgres.png";
import sqliteLogoUrl from "../../resources/db-logo/sqlite.png";
import jdbcLogoUrl from "../../resources/db-logo/jdbc.png";
import type { DatabaseType } from "../types";

export const DATABASE_LOGOS: Partial<Record<DatabaseType, string>> = {
  mysql: mysqlLogoUrl,
  postgres: postgresLogoUrl,
  sqlite: sqliteLogoUrl,
  jdbc: jdbcLogoUrl
};
