// ABOUTME: Verifies docker setup snippets for hosted push and proxy pull flows.
import { describe, expect, it } from "vitest";
import { createDockerSetupSnippets } from "../DockerRepositoryHelpers";

function repository(kind: string | null) {
  return {
    id: "repo-1",
    storage_name: "primary",
    storage_id: "storage-1",
    name: "docker-hosted",
    repository_type: "docker",
    repository_kind: kind,
    active: true,
    visibility: "Private",
    updated_at: "2026-03-26T00:00:00Z",
    created_at: "2026-03-26T00:00:00Z",
    auth_enabled: true,
    storage_usage_bytes: null,
    storage_usage_updated_at: null,
  } as never;
}

describe("DockerRepositoryHelpers", () => {
  it("builds login/tag/push guidance for hosted registries", () => {
    const snippets = createDockerSetupSnippets(repository("hosted"));

    expect(snippets).toHaveLength(1);
    expect(snippets[0]?.code).toContain("docker login");
    expect(snippets[0]?.code).toContain("docker tag");
    expect(snippets[0]?.code).toContain("docker push");
    expect(snippets[0]?.code).toContain("/repositories/primary/docker-hosted");
  });

  it("builds login/pull guidance for proxy registries", () => {
    const snippets = createDockerSetupSnippets(repository("proxy"));

    expect(snippets).toHaveLength(1);
    expect(snippets[0]?.code).toContain("docker login");
    expect(snippets[0]?.code).toContain("docker pull");
    expect(snippets[0]?.code).not.toContain("docker push");
  });
});
