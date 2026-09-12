package com.mingo.datanexa.jdbc;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ArrayNode;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.apache.maven.repository.internal.MavenRepositorySystemUtils;
import org.eclipse.aether.DefaultRepositorySystemSession;
import org.eclipse.aether.RepositorySystem;
import org.eclipse.aether.artifact.Artifact;
import org.eclipse.aether.artifact.DefaultArtifact;
import org.eclipse.aether.collection.CollectRequest;
import org.eclipse.aether.graph.Dependency;
import org.eclipse.aether.impl.DefaultServiceLocator;
import org.eclipse.aether.repository.LocalRepository;
import org.eclipse.aether.repository.RemoteRepository;
import org.eclipse.aether.resolution.DependencyRequest;
import org.eclipse.aether.resolution.DependencyResult;
import org.eclipse.aether.resolution.ArtifactResult;
import org.eclipse.aether.spi.connector.RepositoryConnectorFactory;
import org.eclipse.aether.spi.connector.transport.TransporterFactory;
import org.eclipse.aether.connector.basic.BasicRepositoryConnectorFactory;
import org.eclipse.aether.transport.file.FileTransporterFactory;
import org.eclipse.aether.transport.http.HttpTransporterFactory;
import org.eclipse.aether.util.artifact.JavaScopes;
import org.eclipse.aether.util.filter.DependencyFilterUtils;

import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.util.Collections;
import java.util.HashSet;
import java.util.Set;

/** Resolves runtime JDBC dependencies without invoking the Maven CLI. */
final class MavenDependencyResolver {
    private static final int PROTOCOL_VERSION = 1;
    private static final String DEFAULT_REPOSITORY = "https://repo.maven.apache.org/maven2/";
    private static final ObjectMapper JSON = new ObjectMapper();

    private MavenDependencyResolver() {
    }

    static ObjectNode resolve(String requestId, JsonNode request) throws Exception {
        String coordinate = required(request, "maven_coordinate");
        String repository = request.path("maven_repository").asText(DEFAULT_REPOSITORY).trim();
        String localRepository = required(request, "maven_local_repository");
        String outputDirectory = required(request, "maven_output_directory");

        Path localPath = Paths.get(localRepository);
        Path outputPath = Paths.get(outputDirectory);
        Files.createDirectories(localPath);
        Files.createDirectories(outputPath);

        RepositorySystem system = repositorySystem();
        DefaultRepositorySystemSession session = MavenRepositorySystemUtils.newSession();
        session.setLocalRepositoryManager(
            system.newLocalRepositoryManager(session, new LocalRepository(localPath.toFile()))
        );

        Artifact rootArtifact = parseCoordinate(coordinate);
        Dependency rootDependency = new Dependency(rootArtifact, JavaScopes.RUNTIME);
        RemoteRepository remote = new RemoteRepository.Builder(
            "datanexa-selected", "default", repository
        ).build();
        CollectRequest collect = new CollectRequest(rootDependency, Collections.singletonList(remote));
        DependencyRequest dependencyRequest = new DependencyRequest(
            collect,
            DependencyFilterUtils.classpathFilter(JavaScopes.RUNTIME)
        );
        DependencyResult result = system.resolveDependencies(session, dependencyRequest);
        if (!result.getCollectExceptions().isEmpty()) {
            throw new IOException(
                "Maven dependency collection failed: " + result.getCollectExceptions().get(0).getMessage()
            );
        }

        ObjectNode response = JSON.createObjectNode();
        response.put("protocol_version", PROTOCOL_VERSION);
        response.put("request_id", requestId);
        response.put("ok", true);
        ArrayNode artifacts = response.putArray("artifacts");
        Set<String> seenArtifacts = new HashSet<String>();
        Set<String> usedNames = new HashSet<String>();
        copyRuntimeArtifacts(result.getArtifactResults(), outputPath, artifacts, seenArtifacts, usedNames);
        if (artifacts.isEmpty()) {
            throw new IOException("Maven resolution produced no runtime JAR files");
        }
        return response;
    }

    private static RepositorySystem repositorySystem() {
        DefaultServiceLocator locator = MavenRepositorySystemUtils.newServiceLocator();
        locator.addService(RepositoryConnectorFactory.class, BasicRepositoryConnectorFactory.class);
        locator.addService(TransporterFactory.class, FileTransporterFactory.class);
        locator.addService(TransporterFactory.class, HttpTransporterFactory.class);
        locator.setErrorHandler(new DefaultServiceLocator.ErrorHandler() {
            @Override
            public void serviceCreationFailed(Class<?> type, Class<?> impl, Throwable error) {
                throw new IllegalStateException(
                    "Unable to initialize Maven Resolver service " + type.getName(), error
                );
            }
        });
        RepositorySystem system = locator.getService(RepositorySystem.class);
        if (system == null) {
            throw new IllegalStateException("Maven Resolver is unavailable");
        }
        return system;
    }

    private static void copyRuntimeArtifacts(
        Iterable<ArtifactResult> results,
        Path outputDirectory,
        ArrayNode output,
        Set<String> seenArtifacts,
        Set<String> usedNames
    ) throws Exception {
        for (ArtifactResult result : results) {
            Artifact artifact = result.getArtifact();
            if (!result.isResolved() || artifact == null || artifact.getFile() == null
                || !"jar".equalsIgnoreCase(artifact.getExtension())) {
                continue;
            }
            String key = artifact.getGroupId() + ":" + artifact.getArtifactId() + ":"
                + artifact.getExtension() + ":" + artifact.getClassifier() + ":" + artifact.getVersion();
            if (seenArtifacts.add(key)) {
                String fileName = targetFileName(artifact, usedNames);
                Path target = outputDirectory.resolve(fileName);
                Files.copy(artifact.getFile().toPath(), target, StandardCopyOption.REPLACE_EXISTING);
                ObjectNode file = output.addObject();
                file.put("name", fileName);
                file.put("size", Files.size(target));
                file.put("sha256", sha256(target));
            }
        }
    }

    private static String targetFileName(Artifact artifact, Set<String> usedNames) {
        StringBuilder name = new StringBuilder();
        name.append(artifact.getGroupId()).append('-').append(artifact.getArtifactId())
            .append('-').append(artifact.getVersion());
        if (!artifact.getClassifier().isEmpty()) {
            name.append('-').append(artifact.getClassifier());
        }
        name.append('.').append(artifact.getExtension());
        String candidate = name.toString().replaceAll("[^A-Za-z0-9._-]", "_");
        String unique = candidate;
        int suffix = 2;
        while (!usedNames.add(unique)) {
            int extension = candidate.lastIndexOf('.');
            unique = candidate.substring(0, extension) + "-" + suffix + candidate.substring(extension);
            suffix++;
        }
        return unique;
    }

    private static Artifact parseCoordinate(String value) {
        String[] parts = value.trim().split(":", -1);
        if (parts.length == 3) {
            return new DefaultArtifact(parts[0], parts[1], "", "jar", parts[2]);
        }
        if (parts.length == 4) {
            return new DefaultArtifact(parts[0], parts[1], "", parts[2], parts[3]);
        }
        if (parts.length == 5) {
            return new DefaultArtifact(parts[0], parts[1], parts[3], parts[2], parts[4]);
        }
        throw new IllegalArgumentException("Maven coordinate must use groupId:artifactId:version");
    }

    private static String required(JsonNode request, String field) {
        String value = request.path(field).asText("").trim();
        if (value.isEmpty()) {
            throw new IllegalArgumentException(field + " is required");
        }
        return value;
    }

    private static String sha256(Path path) throws Exception {
        MessageDigest digest = MessageDigest.getInstance("SHA-256");
        byte[] buffer = new byte[64 * 1024];
        try (InputStream input = Files.newInputStream(path)) {
            int read;
            while ((read = input.read(buffer)) >= 0) {
                if (read > 0) {
                    digest.update(buffer, 0, read);
                }
            }
        }
        StringBuilder result = new StringBuilder(64);
        for (byte value : digest.digest()) {
            result.append(String.format("%02x", value));
        }
        return result.toString();
    }
}
