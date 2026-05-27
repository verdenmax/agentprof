# Fixture: corrupt

## Purpose
Verify that a single broken JSONL line does not abort the entire file —
the parser must skip it, accumulate a ParseWarning::Json, and continue.

## Scenarios covered
- Line 3 (0-indexed: line 2) is invalid JSON: `this line is intentionally not valid JSON`
- All other lines are valid and must parse normally

## FRs exercised
- FR-1.8 (single-line parse failure does not abort whole file)

## Expected
5 events, 1 parse warning (ParseWarning::Json { line_no: 2, ... }),
meta.is_live=false.
