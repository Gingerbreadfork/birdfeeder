import { num, plural } from "./format";
import type { Kind, WatchEvent } from "./types";

export type Lane =
  | "star"
  | "follower"
  | "issue"
  | "pr"
  | "inbox"
  | "fork"
  | "downloads"
  | "release";

/** The rows of the activity grid. The order keeps similar colours apart. */
export const LANES: {
  id: Lane;
  label: string;
  /** A shorter name for tight spaces. */
  chip?: string;
  unit: [string, string];
  kinds: Kind[];
}[] = [
  { id: "star", label: "Stars", unit: ["star", "stars"], kinds: ["star", "unstar"] },
  { id: "follower", label: "Followers", unit: ["follower", "followers"], kinds: ["follower", "unfollow"] },
  { id: "issue", label: "Issues", unit: ["issue", "issues"], kinds: ["issue"] },
  { id: "pr", label: "Pull requests", chip: "PRs", unit: ["pull request", "pull requests"], kinds: ["pr"] },
  { id: "inbox", label: "Inbox", unit: ["notification", "notifications"], kinds: ["inbox"] },
  { id: "fork", label: "Forks", unit: ["fork", "forks"], kinds: ["fork"] },
  { id: "downloads", label: "Downloads", unit: ["download", "downloads"], kinds: ["downloads"] },
  { id: "release", label: "Releases", unit: ["release", "releases"], kinds: ["release"] },
];

const LANE_OF: Partial<Record<Kind, Lane>> = {
  star: "star",
  unstar: "star",
  follower: "follower",
  unfollow: "follower",
  issue: "issue",
  pr: "pr",
  inbox: "inbox",
  fork: "fork",
  downloads: "downloads",
  release: "release",
};

const MILESTONE_LANE: Record<string, Lane> = {
  stars: "star",
  downloads: "downloads",
  followers: "follower",
};

export function laneColor(lane: Lane | null): string {
  return lane ? `var(--k-${lane})` : "var(--k-other)";
}

/** Followers a visitor needs before they are pointed out, and before they count as rare. */
const NOTED = 100;
const RARE = 1000;

export function isRare(e: WatchEvent): boolean {
  return (e.actor_followers ?? 0) >= RARE && (e.kind === "star" || e.kind === "fork" || e.kind === "follower");
}

/** Events worth a little ceremony: milestones, records, life-list entries and rare visitors. */
export function isSpecial(e: WatchEvent): boolean {
  return (
    ["milestone", "sighting", "record", "story"].includes(e.kind) ||
    Boolean(e.meta?.sighting) ||
    isRare(e)
  );
}

/** Kinds marked with the bird instead of a square, since no event colour is theirs. */
export function hasBirdMarker(e: WatchEvent): boolean {
  return e.kind === "sighting" || e.kind === "record" || e.kind === "story";
}

export function eventColor(e: WatchEvent): string {
  if (e.kind === "sighting" || e.kind === "record" || e.kind === "story") return "var(--seed)";
  if (e.kind === "trend") return laneColor("star");
  if (e.kind === "milestone") {
    const lane = MILESTONE_LANE[e.subject ?? ""];
    return lane ? laneColor(lane) : "var(--seed)";
  }
  return laneColor(LANE_OF[e.kind] ?? null);
}

/** A run of sentence text: plain, emphasised, a code-like identifier, or a small label. */
export type Part = string | { strong: string } | { code: string } | { tag: string };

export interface Described {
  parts: Part[];
  /** A line of its own for news the event carries, such as a new life-list entry. */
  note?: string;
  /** A second line, such as an issue title. */
  detail?: Part[];
  /** Quoted text from a comment. */
  quote?: string;
  /** Whether the actor's avatar should lead the sentence. */
  byActor: boolean;
}

/** Drops the owner from a repo name when it belongs to the signed-in user. */
export function repoName(full: string | null, login: string | null): string {
  if (!full) return "a repository";
  const [owner, name] = full.split("/");
  return name && login && owner.toLowerCase() === login.toLowerCase() ? name : full;
}

const INBOX_VERB: Record<string, string> = {
  mention: "mentioned you in",
  team_mention: "mentioned your team in",
  review_requested: "asked for your review in",
  assign: "assigned you in",
  comment: "replied in",
  author: "replied on your thread in",
  subscribed: "commented in",
};

const INBOX_HEADLINE: Record<string, string> = {
  mention: "You were mentioned in",
  team_mention: "Your team was mentioned in",
  review_requested: "Review requested in",
  assign: "You were assigned in",
  ci_activity: "Workflow run finished in",
  security_alert: "Security alert in",
  state_change: "Thread status changed in",
  invitation: "Invitation to",
  approval_requested: "Deployment approval requested in",
};

/** A line about who a visitor is, for the ones with a following of their own. */
function introduction(e: WatchEvent): Part[] | undefined {
  const followers = e.actor_followers ?? 0;
  if (followers < NOTED) return undefined;
  const company = e.actor_company?.trim().replace(/^@/, "");
  const line = `${num(followers)} followers${company ? `, works at ${company}` : ""}`;
  return followers >= RARE ? [{ tag: "Rare bird." }, ` ${line}`] : [line];
}

/**
 * How an event reads in the activity list. An event that ticked something off the
 * life list says so on a line of its own.
 */
export function describe(e: WatchEvent, login: string | null): Described {
  const said = sentence(e, login);
  const sighting = e.meta?.sighting as string | undefined;
  if (!sighting) return said;
  // The note already names the sighting, so a label that repeats it is dropped.
  const rest = (said.detail ?? []).filter((part) => !(typeof part === "object" && "tag" in part));
  if (typeof rest[0] === "string") rest[0] = rest[0].trimStart();
  return {
    ...said,
    note: `New on your life list: ${sighting}`,
    detail: rest.length ? rest : undefined,
  };
}

function sentence(e: WatchEvent, login: string | null): Described {
  const repo = { strong: repoName(e.repo, login) };
  const actor = { strong: e.actor ?? "Someone" };
  const titled = (): Part[] | undefined =>
    e.subject ? [e.number ? `#${e.number} ` : "", e.subject] : undefined;

  switch (e.kind) {
    case "star":
      return e.actor
        ? { parts: [actor, " starred ", repo], detail: introduction(e), byActor: true }
        : { parts: [repo, ` gained ${plural(e.count, "more star")}`], byActor: false };
    case "unstar":
      return { parts: [repo, ` lost ${plural(e.count, "star")}`], byActor: false };
    case "issue":
      return { parts: [actor, " opened an issue in ", repo], detail: titled(), byActor: true };
    case "pr":
      return { parts: [actor, " opened a pull request in ", repo], detail: titled(), byActor: true };
    case "fork":
      return { parts: [actor, " forked ", repo], detail: introduction(e), byActor: true };
    case "release": {
      const name = e.meta?.name as string | undefined;
      return {
        parts: [repo, " published ", { code: e.subject ?? "a release" }],
        detail: name && name !== e.subject ? [name] : undefined,
        byActor: false,
      };
    }
    case "downloads": {
      const per = Object.entries((e.meta?.releases ?? {}) as Record<string, number>)
        .filter(([, n]) => n > 0)
        .sort((a, b) => b[1] - a[1]);
      const detail: Part[] = [];
      for (const [tag, n] of per.slice(0, 4)) {
        if (detail.length) detail.push(", ");
        detail.push({ code: tag }, ` +${num(n)}`);
      }
      if (per.length > 4) detail.push(`, and ${per.length - 4} more`);
      return {
        parts: [repo, ` was downloaded ${e.count === 1 ? "once" : `${num(e.count)} times`}`],
        detail: detail.length ? detail : undefined,
        byActor: false,
      };
    }
    case "follower":
      return { parts: [actor, " followed you"], detail: introduction(e), byActor: true };
    case "unfollow":
      return { parts: [actor, " unfollowed you"], byActor: false };
    case "milestone": {
      if (e.subject === "streak") {
        return { parts: [`You reached a ${num(e.count)}-day contribution streak`], byActor: false };
      }
      const what =
        e.subject === "downloads"
          ? plural(e.count, "download")
          : e.subject === "followers"
            ? plural(e.count, "follower")
            : e.subject === "all-stars"
              ? `${plural(e.count, "star")} across your repositories`
              : plural(e.count, "star");
      return {
        parts: e.repo ? [repo, ` reached ${what}`] : [`You reached ${what}`],
        byActor: false,
      };
    }
    case "sighting": {
      const note = (e.meta?.detail ?? e.meta?.how) as string | undefined;
      return {
        parts: ["New on your life list: ", { strong: e.subject ?? "a sighting" }],
        detail: note ? [note] : undefined,
        byActor: false,
      };
    }
    case "record": {
      const previous = e.meta?.previous as number | undefined;
      return {
        parts: [`A new best day: ${plural(e.count, "star")}`],
        detail: previous ? [`The record was ${num(previous)}.`] : undefined,
        byActor: false,
      };
    }
    case "trend": {
      const usual = e.meta?.usual as number | undefined;
      return {
        parts: [repo, ` is picking up: ${plural(e.count, "star")} this week`],
        detail: usual ? [`Up from about ${num(usual)} a week.`] : undefined,
        byActor: false,
      };
    }
    case "story": {
      const comments = (e.meta?.comments as number | undefined) ?? 0;
      return {
        parts: [repo, " is on Hacker News"],
        detail: [`${plural(e.count, "point")} and ${plural(comments, "comment")}. `, e.subject ?? ""],
        byActor: false,
      };
    }
    case "traffic":
      return {
        parts: [repo, ` had ${plural(e.count, "view")} in one day`],
        detail: e.subject ? ["Mostly from ", e.subject] : undefined,
        byActor: false,
      };
    case "inbox": {
      const reason = (e.meta?.reason as string) ?? "";
      const verb = INBOX_VERB[reason];
      const parts: Part[] =
        e.actor && verb
          ? [actor, ` ${verb} `, repo]
          : [`${INBOX_HEADLINE[reason] ?? "New activity in"} `, repo];
      return {
        parts,
        detail: titled(),
        quote: (e.meta?.snippet as string | null) ?? undefined,
        byActor: Boolean(e.actor && verb),
      };
    }
  }
}
