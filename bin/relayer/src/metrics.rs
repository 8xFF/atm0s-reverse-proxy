use metrics::{describe_counter, describe_gauge, describe_histogram};

// this is for online agent counting
pub const METRICS_AGENT_LIVE: &str = "atm0s_agent_live";
pub const METRICS_AGENT_HISTOGRAM: &str = "atm0s_agent_histogram";
pub const METRICS_AGENT_COUNT: &str = "atm0s_agent_count";

pub const METRICS_PROXY_AGENT_SESSION_LIVE: &str = "atm0s_proxy_agent_session_live";
pub const METRICS_PROXY_AGENT_SESSION_INSERT: &str = "atm0s_proxy_agent_session_insert";
pub const METRICS_PROXY_AGENT_SESSION_REMOVE: &str = "atm0s_proxy_agent_session_remove";

// this is for proxy from agent counting (incoming)
pub const METRICS_PROXY_AGENT_LIVE: &str = "atm0s_proxy_agent_live";
pub const METRICS_PROXY_AGENT_COUNT: &str = "atm0s_proxy_agent_count";
pub const METRICS_PROXY_AGENT_HISTOGRAM: &str = "atm0s_proxy_agent_histogram";
pub const METRICS_PROXY_AGENT_ERROR_COUNT: &str = "atm0s_proxy_agent_error_count";

// this is for http proxy counting (incoming)
pub const METRICS_PROXY_HTTP_LIVE: &str = "atm0s_proxy_http_live";
pub const METRICS_PROXY_HTTP_COUNT: &str = "atm0s_proxy_http_count";
pub const METRICS_PROXY_HTTP_ERROR_COUNT: &str = "atm0s_proxy_http_error_count";

// this is for incoming proxy from outside
pub const METRICS_PROXY_OUTSIDE_LIVE: &str = "atm0s_proxy_outside_live";
pub const METRICS_PROXY_OUTSIDE_COUNT: &str = "atm0s_proxy_outside_count";
pub const METRICS_PROXY_OUTSIDE_ERROR_COUNT: &str = "atm0s_proxy_outside_error_count";

// this is for incoming proxy from cluster
pub const METRICS_PROXY_CLUSTER_LIVE: &str = "atm0s_proxy_cluster_live";
pub const METRICS_PROXY_CLUSTER_COUNT: &str = "atm0s_proxy_cluster_count";
pub const METRICS_PROXY_CLUSTER_ERROR_COUNT: &str = "atm0s_proxy_cluster_error_count";

// this is for tunnel from local node to other node (outgoing)
pub const METRICS_TUNNEL_CLUSTER_LIVE: &str = "atm0s_tunnel_cluster_live";
pub const METRICS_TUNNEL_CLUSTER_COUNT: &str = "atm0s_tunnel_cluster_count";
pub const METRICS_TUNNEL_CLUSTER_HISTOGRAM: &str = "atm0s_tunnel_cluster_histogram";
pub const METRICS_TUNNEL_CLUSTER_ERROR_COUNT: &str = "atm0s_tunnel_cluster_error_count";

// this is for tunnel from local node to agent  (outgoing)
pub const METRICS_TUNNEL_AGENT_LIVE: &str = "atm0s_tunnel_agent_live";
pub const METRICS_TUNNEL_AGENT_COUNT: &str = "atm0s_tunnel_agent_count";
pub const METRICS_TUNNEL_AGENT_HISTOGRAM: &str = "atm0s_tunnel_agent_histogram";
pub const METRICS_TUNNEL_AGENT_ERROR_COUNT: &str = "atm0s_tunnel_agent_error_count";

pub fn describe_metrics() {
    p2p::init_metrics();

    // this is for online agent counting
    describe_gauge!(METRICS_AGENT_LIVE, "Live agent count");
    describe_histogram!(METRICS_AGENT_HISTOGRAM, "Incoming agent connection accept time histogram");
    describe_counter!(METRICS_AGENT_COUNT, "Number of connected agents");

    describe_gauge!(METRICS_PROXY_AGENT_SESSION_LIVE, "Live agent session count");
    describe_counter!(METRICS_PROXY_AGENT_SESSION_INSERT, "Number of agent session insert");
    describe_counter!(METRICS_PROXY_AGENT_SESSION_REMOVE, "Number of agent session remove");

    // this is for proxy from agent counting (incoming)
    describe_gauge!(METRICS_PROXY_AGENT_LIVE, "Live incoming proxy from agent to cluster");
    describe_counter!(METRICS_PROXY_AGENT_COUNT, "Number of incoming proxy from agent to cluster");
    describe_histogram!(METRICS_PROXY_AGENT_HISTOGRAM, "Incoming proxy from agent to cluster latency histogram");
    describe_counter!(METRICS_PROXY_AGENT_ERROR_COUNT, "Number of incoming proxy error from agent to cluster");

    // this is for incoming proxy from outside
    describe_gauge!(METRICS_PROXY_OUTSIDE_LIVE, "Live incoming outside proxy");
    describe_counter!(METRICS_PROXY_OUTSIDE_COUNT, "Number of incoming outside proxy");
    describe_counter!(METRICS_PROXY_OUTSIDE_ERROR_COUNT, "Number of incoming outside proxy error");

    // this is for cluster proxy (incoming)
    describe_gauge!(METRICS_PROXY_CLUSTER_LIVE, "Live incoming cluster proxy");
    describe_counter!(METRICS_PROXY_CLUSTER_COUNT, "Number of incoming cluster proxy");
    describe_counter!(METRICS_PROXY_CLUSTER_ERROR_COUNT, "Number of incoming cluster proxy error");

    // this is for tunnel from local node to other node (outgoing)
    describe_gauge!(METRICS_TUNNEL_CLUSTER_LIVE, "Live outgoing tunnel to cluster");
    describe_counter!(METRICS_TUNNEL_CLUSTER_COUNT, "Number of outgoing tunnel to cluster");
    describe_histogram!(METRICS_TUNNEL_CLUSTER_HISTOGRAM, "Outgoing tunnel to cluster latency histogram");
    describe_counter!(METRICS_TUNNEL_CLUSTER_ERROR_COUNT, "Number of outgoing tunnel to cluster error");

    // this is for tunnel from local node to agent  (outgoing)
    describe_gauge!(METRICS_TUNNEL_AGENT_LIVE, "Live outgoing tunnel to agent");
    describe_counter!(METRICS_TUNNEL_AGENT_COUNT, "Number of outgoing tunnel to agent");
    describe_counter!(METRICS_TUNNEL_AGENT_HISTOGRAM, "Outgoing tunnel to agent latency histogram");
    describe_counter!(METRICS_TUNNEL_AGENT_ERROR_COUNT, "Number of outgoing tunnel to agent error");
}
