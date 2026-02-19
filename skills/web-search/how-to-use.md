# Skill: web-search

**Purpose**
Fast, low-latency information retrieval for the Id tier.

**When Ego should call this tool**
- Fact checking, current events, market data.

**Constraints**
- Max 5 results per call.
- No PII in queries.

**Success Pattern**
Return structured JSON + source URLs.

**Failure Recovery**
Fall back to local memory or re-plan.
