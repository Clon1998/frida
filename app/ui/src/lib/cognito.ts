/**
 * The AWS Cognito preset for OAuth2 (PLAN.md §9.2): a user pool domain (prefix or custom
 * domain) and a region give the token and authorization endpoints.
 */
export interface CognitoDomain {
  /** `my-app` (a Cognito prefix) or a custom domain like `auth.example.com`. */
  domain: string;
  region: string;
}

const HOSTED = /^https:\/\/([^.]+)\.auth\.([a-z0-9-]+)\.amazoncognito\.com\/oauth2\/token$/;
const CUSTOM = /^https:\/\/([^/]+)\/oauth2\/token$/;

export function cognitoUrls({ domain, region }: CognitoDomain): {
  tokenUrl: string;
  authUrl: string;
} {
  const host = domain.includes(".")
    ? domain.trim()
    : `${domain.trim()}.auth.${region.trim()}.amazoncognito.com`;
  return {
    tokenUrl: `https://${host}/oauth2/token`,
    authUrl: `https://${host}/oauth2/authorize`,
  };
}

/** Reads domain and region back from a token URL (empty when it isn't a Cognito URL). */
export function parseCognito(tokenUrl: string): CognitoDomain {
  const hosted = HOSTED.exec(tokenUrl);
  if (hosted) return { domain: hosted[1] ?? "", region: hosted[2] ?? "" };
  const custom = CUSTOM.exec(tokenUrl);
  if (custom) return { domain: custom[1] ?? "", region: "" };
  return { domain: "", region: "" };
}
