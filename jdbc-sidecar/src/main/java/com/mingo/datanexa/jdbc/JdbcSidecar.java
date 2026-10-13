package com.mingo.datanexa.jdbc;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;

import java.io.BufferedReader;
import java.io.BufferedWriter;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.sql.Connection;
import java.sql.DatabaseMetaData;
import java.sql.Driver;
import java.sql.ResultSet;
import java.sql.ResultSetMetaData;
import java.sql.SQLException;
import java.sql.Statement;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Base64;
import java.util.Collections;
import java.util.Comparator;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Properties;
import java.util.Set;
import java.util.ServiceConfigurationError;
import java.util.ServiceLoader;
import java.util.regex.Pattern;
import java.util.stream.Collectors;

public final class JdbcSidecar {
    private static final int PROTOCOL_VERSION = 1;
    private static final ObjectMapper JSON = new ObjectMapper();
    private static final Pattern URL_SECRET = Pattern.compile(
        "(?i)(password|passwd|pwd|token|secret)=([^&;\\s]+)"
    );
    private static final Pattern URL_USER_INFO = Pattern.compile(
        "(?i)((?:[A-Za-z][A-Za-z0-9+.-]*:)+//)([^/@\\s:]+):([^/@\\s]+)@"
    );
    private static URLClassLoader sharedLoader;
    private static String sharedDriverKey = "";
    private static List<Driver> sharedDrivers = Collections.emptyList();
    private static String sharedConnectionKey = "";
    private static Connection sharedConnection;

    private JdbcSidecar() {
    }

    public static void main(String[] args) throws Exception {
        try (
            BufferedReader reader = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
            BufferedWriter writer = new BufferedWriter(new OutputStreamWriter(System.out, StandardCharsets.UTF_8))
        ) {
            String line;
            while ((line = reader.readLine()) != null) {
                if (line.trim().isEmpty()) {
                    continue;
                }
                ObjectNode response = handle(line);
                writer.write(JSON.writeValueAsString(response));
                writer.newLine();
                writer.flush();
                if (response.path("_datanexa_close").asBoolean(false)) {
                    break;
                }
            }
        } finally {
            closeSharedConnection();
            closeSharedLoader();
        }
    }

    private static ObjectNode handle(String line) {
        String requestId = "unknown";
        String jdbcUrl = "";
        String password = "";
        try {
            JsonNode request = JSON.readTree(line);
            requestId = text(request, "request_id");
            jdbcUrl = text(request, "jdbc_url");
            password = text(request, "password");
            if (request.path("protocol_version").asInt(-1) != PROTOCOL_VERSION) {
                throw new SidecarException("protocol_mismatch", "Unsupported JDBC sidecar protocol version");
            }

            String action = text(request, "action");
            if ("close".equals(action)) {
                closeSharedConnection();
                closeSharedLoader();
                ObjectNode response = success(requestId);
                response.put("_datanexa_close", true);
                return response;
            }
            if ("resolve_maven".equals(action)) {
                try {
                    return MavenDependencyResolver.resolve(requestId, request);
                } catch (Throwable error) {
                    throw new SidecarException("maven_resolution_failed", message(error));
                }
            }
            if ("inspect".equals(action)) {
                return inspect(requestId, request);
            }
            if ("test_connection".equals(action)) {
                return testConnection(requestId, request);
            }
            if ("list_schema".equals(action)) {
                return listSchema(requestId, request);
            }
            if ("describe_table".equals(action)) {
                return describeTable(requestId, request);
            }
            if ("query".equals(action)) {
                return query(requestId, request);
            }
            throw new SidecarException("unsupported_action", "Unsupported JDBC sidecar action");
        } catch (Throwable error) {
            return failure(requestId, errorCode(error), sanitize(message(error), jdbcUrl, password));
        }
    }

    private static ObjectNode inspect(String requestId, JsonNode request) throws Exception {
        try (URLClassLoader loader = bundleClassLoader(text(request, "bundle_path"))) {
            List<Driver> drivers = loadDrivers(loader);
            ObjectNode response = success(requestId);
            ArrayNode classes = response.putArray("driver_classes");
            for (Driver driver : drivers) {
                classes.add(driver.getClass().getName());
            }
            return response;
        }
    }

    private static ObjectNode testConnection(String requestId, JsonNode request) throws Exception {
        String jdbcUrl = text(request, "jdbc_url");
        if (jdbcUrl.trim().isEmpty() || !jdbcUrl.startsWith("jdbc:")) {
            throw new SidecarException("invalid_url", "A JDBC URL beginning with jdbc: is required");
        }
        // The cached shared connection would answer instantly and hide both real
        // connect latency and dead connections, so a test always reconnects.
        closeSharedConnection();
        Connection connection = openConnection(request);
        DatabaseMetaData metadata = connection.getMetaData();
        ObjectNode response = success(requestId);
        response.put("database_product", safeMetadata(metadata.getDatabaseProductName()));
        response.put("database_version", safeMetadata(metadata.getDatabaseProductVersion()));
        response.put("driver_name", safeMetadata(metadata.getDriverName()));
        response.put("driver_version", safeMetadata(metadata.getDriverVersion()));
        return response;
    }

    private static ObjectNode listSchema(String requestId, JsonNode request) throws Exception {
        Connection connection = openConnection(request);
        {
            DatabaseMetaData metadata = connection.getMetaData();
            ObjectNode response = success(requestId);
            ArrayNode tables = response.putArray("tables");
            Set<String> seen = new HashSet<String>();
            try (ResultSet result = metadata.getTables(null, null, "%", new String[] {
                "TABLE", "VIEW", "SYSTEM TABLE", "GLOBAL TEMPORARY", "LOCAL TEMPORARY", "SYNONYM"
            })) {
                while (result.next()) {
                    String schema = result.getString("TABLE_SCHEM");
                    String name = result.getString("TABLE_NAME");
                    String type = result.getString("TABLE_TYPE");
                    if (name == null || name.trim().isEmpty()) {
                        continue;
                    }
                    String key = String.valueOf(schema) + "\u0000" + name + "\u0000" + type;
                    if (!seen.add(key)) {
                        continue;
                    }
                    ObjectNode table = tables.addObject();
                    if (schema != null) {
                        table.put("schema", schema);
                    } else {
                        table.putNull("schema");
                    }
                    table.put("name", name);
                    table.put("table_type", type == null || type.trim().isEmpty() ? "TABLE" : type);
                }
            }
            return response;
        }
    }

    private static ObjectNode describeTable(String requestId, JsonNode request) throws Exception {
        String schema = optionalText(request, "schema");
        String tableName = text(request, "table");
        if (tableName.trim().isEmpty()) {
            throw new SidecarException("invalid_table", "A table name is required");
        }
        Connection connection = openConnection(request);
        {
            DatabaseMetaData metadata = connection.getMetaData();
            String[] target = resolveTable(metadata, connection.getCatalog(), schema, tableName);
            Set<String> primaryKeys = new HashSet<String>();
            try (ResultSet result = metadata.getPrimaryKeys(target[0], target[1], target[2])) {
                while (result.next()) {
                    if (!matchesTable(result, target)) {
                        continue;
                    }
                    String column = result.getString("COLUMN_NAME");
                    if (column != null) {
                        primaryKeys.add(column);
                    }
                }
            }
            ObjectNode response = success(requestId);
            ArrayNode columns = response.putArray("columns");
            try (ResultSet result = metadata.getColumns(
                target[0], metadataPattern(metadata, target[1]), metadataPattern(metadata, target[2]), "%"
            )) {
                while (result.next()) {
                    if (!matchesTable(result, target)) {
                        continue;
                    }
                    String name = result.getString("COLUMN_NAME");
                    if (name == null || name.trim().isEmpty()) {
                        continue;
                    }
                    ObjectNode column = columns.addObject();
                    column.put("name", name);
                    column.put("data_type", safeMetadata(result.getString("TYPE_NAME")));
                    column.put("nullable", "YES".equalsIgnoreCase(result.getString("IS_NULLABLE")));
                    column.put("primary_key", primaryKeys.contains(name));
                }
            }
            return response;
        }
    }

    private static String metadataPattern(DatabaseMetaData metadata, String value) throws SQLException {
        if (value == null) {
            return null;
        }
        String escape = metadata.getSearchStringEscape();
        if (escape == null || escape.isEmpty()) {
            // Exact identity filtering below also covers drivers without pattern escaping.
            return value;
        }
        return value.replace(escape, escape + escape).replace("_", escape + "_").replace("%", escape + "%");
    }

    private static String[] resolveTable(DatabaseMetaData metadata, String catalog, String schema, String table)
        throws SQLException {
        String[] target = null;
        try (ResultSet result = metadata.getTables(
            catalog, metadataPattern(metadata, schema), metadataPattern(metadata, table), null
        )) {
            while (result.next()) {
                String[] candidate = {
                    result.getString("TABLE_CAT"), result.getString("TABLE_SCHEM"), result.getString("TABLE_NAME")
                };
                if (!table.equals(candidate[2])
                    || (catalog != null && !catalog.equals(candidate[0]))
                    || (schema != null && !schema.equals(candidate[1]))) {
                    continue;
                }
                if (target != null && !Arrays.equals(target, candidate)) {
                    throw new SidecarException("ambiguous_table", "Specify a schema or catalog to identify one table");
                }
                target = candidate;
            }
        }
        if (target == null) {
            throw new SidecarException("table_not_found", "The requested table was not found");
        }
        return target;
    }

    private static boolean matchesTable(ResultSet result, String[] target) throws SQLException {
        return Objects.equals(target[0], result.getString("TABLE_CAT"))
            && Objects.equals(target[1], result.getString("TABLE_SCHEM"))
            && Objects.equals(target[2], result.getString("TABLE_NAME"));
    }

    private static ObjectNode query(String requestId, JsonNode request) throws Exception {
        String sql = text(request, "sql");
        if (sql.trim().isEmpty()) {
            throw new SidecarException("invalid_sql", "A SQL statement is required");
        }
        int maxRows = Math.max(1, Math.min(5000, request.path("max_rows").asInt(500)));
        long maxBytes = Math.max(64 * 1024L, Math.min(8 * 1024 * 1024L, request.path("max_result_bytes").asLong(1024 * 1024L)));
        Connection connection = openConnection(request);
        try (Statement statement = connection.createStatement()) {
            try {
                connection.setReadOnly(true);
            } catch (SQLException ignored) {
                // Some drivers only accept read-only mode in connection properties.
            }
            int timeoutSeconds = Math.max(1, Math.min(60, request.path("query_timeout_ms").asInt(8000) / 1000));
            try {
                statement.setQueryTimeout(timeoutSeconds);
            } catch (SQLException ignored) {
                // Some drivers (e.g. read-only CSV drivers) do not support query timeouts.
            }
            try (ResultSet result = statement.executeQuery(sql)) {
                ResultSetMetaData metadata = result.getMetaData();
                ObjectNode response = success(requestId);
                ArrayNode columns = response.putArray("column_names");
                List<String> names = uniqueColumnNames(metadata);
                for (String name : names) {
                    columns.add(name);
                }
                ArrayNode rows = response.putArray("rows");
                ArrayNode truncatedCells = response.putArray("truncated_cells");
                boolean truncated = false;
                String truncationReason = null;
                while (result.next()) {
                    if (rows.size() >= maxRows) {
                        truncated = true;
                        truncationReason = "row_limit";
                        break;
                    }
                    int previousTruncations = truncatedCells.size();
                    ObjectNode row = rows.addObject();
                    for (int index = 1; index <= names.size(); index++) {
                        String column = names.get(index - 1);
                        row.set(column, jdbcValue(result.getObject(index), truncatedCells, rows.size() - 1, column));
                    }
                    if (JSON.writeValueAsBytes(response).length > maxBytes) {
                        rows.remove(rows.size() - 1);
                        while (truncatedCells.size() > previousTruncations) {
                            truncatedCells.remove(truncatedCells.size() - 1);
                        }
                        truncated = true;
                        truncationReason = "result_size";
                        break;
                    }
                }
                if (truncatedCells.size() > 0) {
                    truncated = true;
                    if (truncationReason == null) {
                        truncationReason = "cell_size";
                    }
                }
                response.put("row_count", rows.size());
                response.put("truncated", truncated);
                if (truncationReason == null) {
                    response.putNull("truncation_reason");
                } else {
                    response.put("truncation_reason", truncationReason);
                }
                response.put("returned_bytes", JSON.writeValueAsBytes(response).length);
                return response;
            }
        }
    }

    private static Connection openConnection(JsonNode request) throws Exception {
        String jdbcUrl = text(request, "jdbc_url");
        if (jdbcUrl.trim().isEmpty() || !jdbcUrl.startsWith("jdbc:")) {
            throw new SidecarException("invalid_url", "A JDBC URL beginning with jdbc: is required");
        }
        ensureDriverContext(request);
        String key = connectionKey(request);
        if (sharedConnection != null && key.equals(sharedConnectionKey) && !isClosed(sharedConnection)) {
            return sharedConnection;
        }
        closeSharedConnection();

        Properties properties = new Properties();
        String username = text(request, "username");
        String password = text(request, "password");
        if (!username.isEmpty()) {
            properties.setProperty("user", username);
        }
        if (!password.isEmpty()) {
            properties.setProperty("password", password);
        }
        SQLException lastSqlError = null;
        for (Driver driver : sharedDrivers) {
            try {
                if (!driver.acceptsURL(jdbcUrl)) {
                    continue;
                }
                Connection connection = driver.connect(jdbcUrl, properties);
                if (connection != null) {
                    sharedConnection = connection;
                    sharedConnectionKey = key;
                    return connection;
                }
            } catch (SQLException error) {
                lastSqlError = error;
            }
        }
        if (lastSqlError != null) {
            throw lastSqlError;
        }
        throw new SidecarException("url_not_accepted", "The installed JDBC driver does not accept this JDBC URL");
    }

    private static void ensureDriverContext(JsonNode request) throws Exception {
        String key = driverKey(request);
        if (sharedLoader != null && key.equals(sharedDriverKey)) {
            return;
        }
        closeSharedConnection();
        closeSharedLoader();
        URLClassLoader loader = bundleClassLoader(text(request, "bundle_path"));
        List<Driver> drivers = loadDrivers(loader);
        String explicitClass = text(request, "driver_class").trim();
        if (!explicitClass.isEmpty()) {
            drivers = Collections.singletonList(instantiateDriver(loader, explicitClass));
        }
        if (drivers.isEmpty()) {
            throw new SidecarException(
                "driver_not_found",
                "No JDBC driver was discovered. Configure Driver Class explicitly for this connection."
            );
        }
        sharedLoader = loader;
        sharedDriverKey = key;
        sharedDrivers = drivers;
    }

    private static String driverKey(JsonNode request) {
        return text(request, "bundle_path") + "|" + text(request, "driver_class");
    }

    private static String connectionKey(JsonNode request) {
        return driverKey(request) + "|" + text(request, "jdbc_url") + "|"
            + text(request, "username") + "|" + text(request, "password");
    }

    private static boolean isClosed(Connection connection) {
        try {
            return connection.isClosed();
        } catch (SQLException error) {
            return true;
        }
    }

    private static void closeSharedConnection() {
        if (sharedConnection != null) {
            try {
                sharedConnection.close();
            } catch (SQLException ignored) {
            }
            sharedConnection = null;
            sharedConnectionKey = "";
        }
    }

    private static void closeSharedLoader() {
        if (sharedLoader != null) {
            try {
                sharedLoader.close();
            } catch (Exception ignored) {
            }
            sharedLoader = null;
            sharedDriverKey = "";
            sharedDrivers = Collections.emptyList();
        }
    }

    private static List<String> uniqueColumnNames(ResultSetMetaData metadata) throws SQLException {
        List<String> names = new ArrayList<String>();
        Set<String> used = new LinkedHashSet<String>();
        for (int index = 1; index <= metadata.getColumnCount(); index++) {
            String base = metadata.getColumnLabel(index);
            if (base == null || base.trim().isEmpty()) {
                base = "column_" + index;
            }
            String name = base;
            int suffix = 2;
            while (!used.add(name)) {
                name = base + "_" + suffix++;
            }
            names.add(name);
        }
        return names;
    }

    private static JsonNode jdbcValue(Object value, ArrayNode truncatedCells, int row, String column)
        throws SQLException {
        if (value == null) {
            return JSON.nullNode();
        }
        if (value instanceof byte[]) {
            return JSON.getNodeFactory().textNode("base64:" + Base64.getEncoder().encodeToString((byte[]) value));
        }
        if (value instanceof java.sql.Blob) {
            java.sql.Blob blob = (java.sql.Blob) value;
            try {
                long originalLength = blob.length();
                int length = (int) Math.min(originalLength, 64 * 1024L);
                byte[] bytes = blob.getBytes(1, length);
                recordCellTruncation(truncatedCells, row, column, originalLength, bytes.length);
                return JSON.getNodeFactory().textNode("base64:" + Base64.getEncoder().encodeToString(bytes));
            } finally {
                blob.free();
            }
        }
        if (value instanceof java.sql.Clob) {
            java.sql.Clob clob = (java.sql.Clob) value;
            try {
                long originalLength = clob.length();
                int length = (int) Math.min(originalLength, 64 * 1024L);
                String content = clob.getSubString(1, length);
                recordCellTruncation(truncatedCells, row, column, originalLength, content.length());
                return JSON.getNodeFactory().textNode(content);
            } finally {
                clob.free();
            }
        }
        if (value instanceof Number || value instanceof Boolean) {
            return JSON.valueToTree(value);
        }
        return JSON.getNodeFactory().textNode(String.valueOf(value));
    }

    private static void recordCellTruncation(ArrayNode cells, int row, String column, long original, long returned) {
        if (original > returned) {
            ObjectNode cell = cells.addObject();
            cell.put("row_index", row);
            cell.put("column", column);
            cell.put("original_length", original);
            cell.put("returned_length", returned);
        }
    }

    private static URLClassLoader bundleClassLoader(String bundlePath) throws Exception {
        if (bundlePath.trim().isEmpty()) {
            throw new SidecarException("invalid_bundle", "JDBC driver bundle path is required");
        }
        Path jarsDirectory = Paths.get(bundlePath).toAbsolutePath().normalize().resolve("jars");
        if (!Files.isDirectory(jarsDirectory)) {
            throw new SidecarException("invalid_bundle", "JDBC driver bundle does not contain a jars directory");
        }
        List<Path> jars;
        try (java.util.stream.Stream<Path> paths = Files.list(jarsDirectory)) {
            jars = paths
                .filter(path -> Files.isRegularFile(path) && path.getFileName().toString().toLowerCase().endsWith(".jar"))
                .sorted(Comparator.comparing(path -> path.getFileName().toString()))
                .collect(Collectors.toList());
        }
        if (jars.isEmpty()) {
            throw new SidecarException("invalid_bundle", "JDBC driver bundle contains no JAR files");
        }
        URL[] urls = new URL[jars.size()];
        for (int index = 0; index < jars.size(); index++) {
            urls[index] = jars.get(index).toUri().toURL();
        }
        return new URLClassLoader(urls, JdbcSidecar.class.getClassLoader());
    }

    private static List<Driver> loadDrivers(ClassLoader loader) {
        Map<String, Driver> drivers = new LinkedHashMap<String, Driver>();
        try {
            for (Driver driver : ServiceLoader.load(Driver.class, loader)) {
                drivers.put(driver.getClass().getName(), driver);
            }
        } catch (ServiceConfigurationError error) {
            throw new SidecarException("driver_load_failed", message(error));
        }
        return new ArrayList<Driver>(drivers.values());
    }

    private static Driver instantiateDriver(ClassLoader loader, String className) throws Exception {
        Class<?> type = Class.forName(className, true, loader);
        Object instance = type.getDeclaredConstructor().newInstance();
        if (!(instance instanceof Driver)) {
            throw new SidecarException("driver_load_failed", "Configured Driver Class does not implement java.sql.Driver");
        }
        return (Driver) instance;
    }

    private static ObjectNode success(String requestId) {
        ObjectNode response = JSON.createObjectNode();
        response.put("protocol_version", PROTOCOL_VERSION);
        response.put("request_id", requestId);
        response.put("ok", true);
        return response;
    }

    private static ObjectNode failure(String requestId, String code, String message) {
        ObjectNode response = JSON.createObjectNode();
        response.put("protocol_version", PROTOCOL_VERSION);
        response.put("request_id", requestId);
        response.put("ok", false);
        ObjectNode error = response.putObject("error");
        error.put("code", code);
        error.put("message", message);
        return response;
    }

    private static String text(JsonNode node, String field) {
        JsonNode value = node.path(field);
        return value.isTextual() ? value.asText() : "";
    }

    private static String optionalText(JsonNode node, String field) {
        String value = text(node, field).trim();
        return value.isEmpty() ? null : value;
    }

    private static String errorCode(Throwable error) {
        if (error instanceof SidecarException) {
            return ((SidecarException) error).code;
        }
        if (error instanceof SQLException) {
            String sqlState = ((SQLException) error).getSQLState();
            if (sqlState != null && sqlState.startsWith("28")) {
                return "authentication_failed";
            }
            if (sqlState != null && sqlState.startsWith("08")) {
                return "connection_lost";
            }
            return "jdbc_error";
        }
        return "sidecar_error";
    }

    private static String message(Throwable error) {
        Throwable cause = error;
        while (cause.getCause() != null && cause.getCause() != cause) {
            cause = cause.getCause();
        }
        String message = cause.getMessage();
        return message == null || message.trim().isEmpty() ? cause.getClass().getSimpleName() : message;
    }

    private static String sanitize(String message, String jdbcUrl, String password) {
        String sanitized = message == null ? "JDBC operation failed" : message;
        if (jdbcUrl != null && !jdbcUrl.isEmpty()) {
            sanitized = sanitized.replace(jdbcUrl, "[JDBC_URL_REDACTED]");
        }
        if (password != null && !password.isEmpty()) {
            sanitized = sanitized.replace(password, "[REDACTED]");
        }
        sanitized = URL_USER_INFO.matcher(sanitized).replaceAll("$1$2:[REDACTED]@");
        sanitized = URL_SECRET.matcher(sanitized).replaceAll("$1=[REDACTED]");
        return sanitized.replaceAll("[\\r\\n\\t]+", " ").trim();
    }

    private static String safeMetadata(String value) {
        if (value == null) {
            return "";
        }
        return value.replaceAll("[\\r\\n\\t]+", " ").trim();
    }

    private static final class SidecarException extends RuntimeException {
        private final String code;

        private SidecarException(String code, String message) {
            super(message);
            this.code = code;
        }
    }
}
