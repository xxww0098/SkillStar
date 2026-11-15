import { expect, test } from "nsh/test";
import { githubOwnerForSkill, sanitizeGithubOwner } from "./repoAvatar";

test("sanitizeGithubOwner trims @ and rejects invalid handles", () => {
  expect(sanitizeGithubOwner("@octocat")).toBe("octocat");
  expect(sanitizeGithubOwner("  vercel-labs  ")).toBe("vercel-labs");
  expect(sanitizeGithubOwner("team_01")).toBe("team_01");
  expect(sanitizeGithubOwner("")).toBe(null);
  expect(sanitizeGithubOwner("-invalid")).toBe(null);
  expect(sanitizeGithubOwner("../passwd")).toBe(null);
  expect(sanitizeGithubOwner("owner/repo")).toBe(null);
  expect(sanitizeGithubOwner("a".repeat(40))).toBe(null);
});

test("githubOwnerForSkill resolves repo owner from source, gitUrl, or author and omits local skills", () => {
  expect(
    githubOwnerForSkill({
      skillType: "hub",
      source: "mattpocock/skills",
      gitUrl: "https://github.com/other/repo.git",
      author: "nobody",
    })
  ).toBe("mattpocock");

  expect(
    githubOwnerForSkill({
      skillType: "hub",
      source: "orchestration",
      gitUrl: "https://github.com/ephemeral-ai-lab/orca.git",
    })
  ).toBe("ephemeral-ai-lab");

  expect(
    githubOwnerForSkill({
      skillType: "hub",
      source: "",
      gitUrl: "git@github.com:vercel-labs/skills.git",
    })
  ).toBe("vercel-labs");

  expect(
    githubOwnerForSkill({
      skillType: "hub",
      source: "remote",
      author: "@octocat",
      gitUrl: "",
    })
  ).toBe("octocat");

  expect(
    githubOwnerForSkill({
      skillType: "local",
      source: "mattpocock/skills",
      author: "mattpocock",
      gitUrl: "https://github.com/mattpocock/skills",
    })
  ).toBe(null);
});
