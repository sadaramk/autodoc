# Requirements Document

## Introduction

A service that accepts jobs and reports on them. Written in Kiro's layout,
which is the one most specifications in the wild actually use.

## Requirements

### Requirement 1

**User Story:** As a client, I want to submit a job, so that work happens without me running it.

#### Acceptance Criteria

1. WHEN a client sends a POST request to `/v1/jobs` with a prompt THEN the system SHALL return HTTP 202
2. WHEN a client polls GET `/v1/jobs/{id}` THEN the system SHALL return the current status
3. WHEN a client requests GET `/v1/jobs/{id}/artifact` THEN the system SHALL return a signed URL
4. WHEN a client sends POST `/v1/jobs/{id}/cancel` THEN the system SHALL stop the job
5. WHEN the queue is full THEN POST `/v1/jobs` SHALL return HTTP 429

### Requirement 2

**User Story:** As an operator, I want the service to be observable, so that I can tell whether it is healthy.

#### Acceptance Criteria

1. WHEN the system is running THEN it SHALL expose `/healthz` and `/metrics` endpoints
2. WHEN errors occur THEN the system SHALL log context without exposing secrets

### Requirement 3

**User Story:** As an operator, I want the service to survive load, so that clients are not dropped.

#### Acceptance Criteria

1. WHEN the queue is full THEN the system SHALL shed load rather than block
2. WHEN a worker dies THEN the system SHALL restart it
