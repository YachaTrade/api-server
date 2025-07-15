# NADS Pump API Server Architecture Specification

## Overview

NADS Pump is a high-performance Rust-based API server designed for a decentralized trading platform. The architecture emphasizes scalability, real-time performance, and robustness suitable for cryptocurrency/DeFi applications.

## Table of Contents

1. [Technology Stack](#technology-stack)
2. [Architecture Overview](#architecture-overview)
3. [Core Components](#core-components)
4. [Module Organization](#module-organization)
5. [Data Layer Architecture](#data-layer-architecture)
6. [Type System Design](#type-system-design)
7. [Security Architecture](#security-architecture)
8. [Performance Optimization](#performance-optimization)
9. [Deployment Architecture](#deployment-architecture)

## Technology Stack

### Core Technologies
- **Language**: Rust (2021 Edition)
- **Web Framework**: Axum 0.7.5
- **Async Runtime**: Tokio 1.38.0
- **Database ORM**: SQLx 0.8.1 with compile-time checked queries
- **API Documentation**: Utoipa (OpenAPI/Swagger)

### Data Storage
- **Primary Database**: PostgreSQL with PgBouncer connection pooling
- **Cache Layer**: Redis with deadpool connection management
- **Object Storage**: AWS S3 for file storage

### Blockchain Integration
- **Web3 Library**: Alloy 0.11.0 for Ethereum interactions
- **Supported Networks**: EVM-compatible chains

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                          Client Layer                            │
│                    (Web3 DApps, Mobile Apps)                     │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                        API Gateway Layer                         │
│                  (CORS, Rate Limiting, Timeout)                  │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Application Layer                           │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐        │
│  │   Auth   │  │  Account │  │  Trading │  │  Social  │  ...   │
│  │  Module  │  │  Module  │  │  Module  │  │  Module  │        │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘        │
└─────────────────────────────────────────────────────────────────┘
                                  │
                                  ▼
┌─────────────────────────────────────────────────────────────────┐
│                         Data Layer                               │
│  ┌─────────────┐  ┌─────────────┐  ┌──────────────┐            │
│  │ PostgreSQL  │  │    Redis    │  │   AWS S3     │            │
│  │  (Primary)  │  │   (Cache)   │  │  (Storage)   │            │
│  └─────────────┘  └─────────────┘  └──────────────┘            │
└─────────────────────────────────────────────────────────────────┘
```

### Architectural Patterns

1. **Layered Architecture**: Clear separation of concerns with distinct layers
2. **Repository Pattern**: Database access abstracted through controller types
3. **Shared State Pattern**: Arc-wrapped state components for thread-safe access
4. **Feature-Based Modules**: Self-contained modules for each business domain

## Core Components

### 1. Application State Management

```rust
pub struct AppState {
    pub postgres: Arc<PostgresDatabase>,
    pub session_redis: Arc<RedisDatabase>,
    pub trade_redis: Arc<RedisDatabase>,
    pub s3_client: Arc<S3Client>,
}
```

The application state is:
- Created once at startup
- Cloned (Arc reference) for each request
- Thread-safe and immutable
- Provides access to all shared resources

### 2. Request Processing Pipeline

1. **Request Reception**: Axum server receives HTTP request
2. **Middleware Processing** (in order):
   - Rate limiting (Governor - currently disabled)
   - Cookie management
   - CORS validation
   - Timeout enforcement (10 seconds)
   - Authentication (for protected routes)
3. **Route Handling**: Request routed to appropriate handler
4. **Business Logic**: Handler processes request using controllers
5. **Response Formation**: Data serialized and returned

### 3. Error Handling Architecture

```rust
pub enum AppError {
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    NotFound(String),
    InternalServerError(String),
    ServiceUnavailable(String),
}
```

- Automatic conversion to HTTP responses
- Consistent error format across API
- Detailed error messages for debugging
- Client-safe error messages

## Module Organization

```
src/
├── main.rs              # Application entry point
├── lib.rs               # Library exports
├── state.rs             # Application state definition
├── config.rs            # Configuration constants
├── middleware.rs        # Authentication middleware
├── cors.rs              # CORS configuration
├── result.rs            # Error types and handling
├── utils.rs             # Utility functions
│
├── db/                  # Database layer
│   ├── mod.rs           # Database module exports
│   ├── postgres/        # PostgreSQL implementation
│   ├── redis/           # Redis implementation
│   └── aws/             # AWS S3 client
│
├── router/              # HTTP routing layer
│   ├── mod.rs           # Router configuration
│   ├── auth/            # Authentication endpoints
│   ├── account/         # User account management
│   ├── token/           # Token operations
│   ├── trade/           # Trading functionality
│   ├── order/           # Order management
│   ├── follow/          # Social following
│   ├── profile/         # User profiles
│   ├── search/          # Search functionality
│   ├── bot/             # Bot operations
│   ├── hype/            # Hype/trending features
│   └── management/      # Admin operations
│
└── types/               # Data models and types
    ├── mod.rs           # Type exports
    ├── common/          # Shared types
    ├── auth/            # Authentication types
    ├── account/         # Account types
    ├── token/           # Token types
    ├── trading/         # Trading types
    ├── social/          # Social types
    └── search/          # Search types
```

### Module Design Principles

1. **Self-Contained Features**: Each router module contains:
   - `mod.rs`: Module exports and router configuration
   - `handler.rs`: HTTP request handlers
   - `path.rs`: Route path definitions

2. **Shared Types**: Common types in `types/common/` for cross-module use

3. **Controller Pattern**: Business logic encapsulated in controller types

## Data Layer Architecture

### Database Schema Design

The database follows a normalized relational design with strategic denormalization for performance:

#### Core Tables
- **account**: User profiles with social metrics
- **token**: Token metadata and creation details
- **market**: Unified market data (CURVE and DEX)
- **position**: User trading positions with P&L
- **swap**: Transaction history
- **chart**: OHLCV price data (partitioned)

#### Performance Tables
- **Count Tables**: Pre-calculated counts (token_count, swap_count)
- **Cache Tables**: Expensive query results (position_top_cache)
- **Event Tables**: Event sourcing for blockchain events

### PostgreSQL Architecture

#### Connection Pooling
```rust
Write Pool: 50 max connections, 5 min connections
Read Pool: 1000 max connections, 10 min connections
```

- **Read/Write Splitting**: Separate pools for primary and replica
- **PgBouncer Optimization**: Transaction pooling mode
- **Statement Caching**: 1000 (write) / 2000 (read) cached statements

#### Partitioning Strategy
```sql
CREATE TABLE chart (...) PARTITION BY HASH (token_id);
-- 16 partitions (chart_0 through chart_15)
```

#### Indexing Strategy
1. **Primary Indexes**: All foreign keys and primary keys
2. **Composite Indexes**: For common query patterns
3. **Partial Indexes**: For filtered queries
4. **Full-Text Indexes**: For search functionality

### Redis Architecture

#### Dual Redis Design
1. **Session Redis**: Authentication and session data
2. **Trade Redis**: Market data and trading cache

#### Caching Strategy
```
Cache Key Patterns:
- token:{token_id}
- order:{order_type}:response:page:{page}_limit:{limit}
- search:tokens:query:{query}:page:{page}:limit:{limit}
```

#### TTL Configuration
- Session data: 24 hours
- Sign-in messages: 3 minutes
- Market data: 1.5 seconds
- Chart data: 5-300 seconds (based on interval)

### Database Triggers

Extensive use of PostgreSQL triggers for:
1. **Real-time Notifications**: pg_notify for WebSocket updates
2. **Count Maintenance**: Automatic count table updates
3. **Business Logic**: Referral rewards, score calculations

## Type System Design

### Design Principles

1. **Strong Typing**: Minimal use of dynamic types
2. **Compile-Time Safety**: SQLx query verification at compile time
3. **Domain Modeling**: Types closely match business domains
4. **Serialization**: Consistent serde patterns

### Common Patterns

```rust
// Smart enum for flexible identification
pub enum Identifier {
    Address(String),
    Nickname(String),
}

// Standardized pagination
pub struct PaginationParams {
    pub page: Option<i32>,
    pub limit: Option<i32>,
    pub direction: Option<String>,
}

// Consistent response wrapping
pub struct TokenResponse {
    pub total_count: i64,
    pub tokens: Vec<TokenWithAccountInfo>,
}
```

### Type Categories

1. **Request Types**: Input validation with serde
2. **Response Types**: Consistent structure with metadata
3. **Domain Types**: Business logic entities
4. **Common Types**: Shared across modules

## Security Architecture

### Authentication Flow

1. **SIWE (Sign-In with Ethereum)**:
   - Nonce generation and storage
   - Message signature verification
   - Session creation with cookie

2. **Session Management**:
   - Redis-backed sessions
   - 24-hour TTL
   - Cookie-based authentication

3. **Middleware Protection**:
   - Selective route protection
   - User injection into request extensions

### Security Measures

1. **CORS Configuration**: Environment-based origin validation
2. **Rate Limiting**: Configured via Governor (currently disabled)
3. **SQL Injection Prevention**: Prepared statements via SQLx
4. **Input Validation**: Strong typing and serde validation
5. **Timeout Protection**: 10-second request timeout

## Performance Optimization

### Caching Strategy

1. **Multi-Level Caching**:
   - Redis for hot data
   - PostgreSQL materialized views
   - Count tables for aggregations

2. **Cache Invalidation**:
   - TTL-based expiration
   - Manual invalidation for critical updates

### Database Optimization

1. **Connection Pooling**: Optimized for serverless environments
2. **Query Optimization**: Compile-time verified queries
3. **Batch Operations**: Reduced round trips
4. **Partitioning**: Hash partitioning for large tables

### Async Performance

1. **Tokio Runtime**: Full async/await support
2. **Concurrent Processing**: Arc-based state sharing
3. **Non-blocking I/O**: All database operations async

## Deployment Architecture

### Environment Support

1. **Standard Deployment**: Standalone server with configurable port
2. **AWS Lambda**: Ready for serverless deployment
3. **Container Support**: Dockerfile provided

### Configuration Management

1. **Environment Variables**: All configuration via env vars
2. **CLI Arguments**: Runtime configuration options
3. **Precedence**: CLI > Environment > Defaults

### Monitoring and Observability

1. **Structured Logging**: Tracing with multiple formats
2. **Error Tracking**: Comprehensive error types
3. **Performance Metrics**: Database query timing

## Best Practices and Conventions

### Code Organization
- Feature-based module structure
- Consistent file naming patterns
- Clear separation of concerns

### Error Handling
- Result types for all fallible operations
- Descriptive error messages
- Graceful degradation

### Database Practices
- Compile-time query verification
- Prepared statements
- Transaction support

### API Design
- RESTful principles
- Consistent response formats
- Comprehensive documentation

## Future Considerations

1. **Scalability**: Current architecture supports horizontal scaling
2. **Monitoring**: Integration with APM tools recommended
3. **Service Mesh**: Ready for microservices evolution
4. **GraphQL**: Could be added alongside REST API

## Conclusion

The NADS Pump API server demonstrates a mature, production-ready architecture suitable for high-performance DeFi applications. The design emphasizes type safety, performance, and maintainability while providing flexibility for future growth.