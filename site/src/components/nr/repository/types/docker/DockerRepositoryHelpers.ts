// ABOUTME: Builds docker login/tag/push snippets for hosted and proxy setups.
// ABOUTME: Keeps CLI guidance next to the repository URL in the setup panel.
import type { CodeSnippet } from "@/components/core/code/code";
import type { RepositoryWithStorageName } from "@/types/repository";
import { createRepositoryRoute } from "@/types/repositoryRoute";

export function createDockerSetupSnippets(
  repository: RepositoryWithStorageName,
): Array<CodeSnippet> {
  const url = createRepositoryRoute(repository);
  const host = url.replace(/^https?:\/\//, "").replace(/\/$/, "");
  const kind = repository.repository_kind?.toLowerCase();
  const image = `${host}/<image>:<tag>`;

  if (kind === "proxy") {
    return [
      {
        name: "Docker",
        language: "bash",
        key: "docker-proxy",
        code: `docker login ${host}\ndocker pull ${image}`,
      },
    ];
  }

  return [
    {
      name: "Docker",
      language: "bash",
      key: "docker-hosted",
      code: `docker login ${host}\ndocker tag <image>:<tag> ${image}\ndocker push ${image}`,
    },
  ];
}
