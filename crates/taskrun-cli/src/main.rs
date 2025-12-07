//! TaskRun CLI - Command line interface for TaskRun control plane.

use clap::{Parser, Subcommand};
use tonic::transport::Channel;

use taskrun_proto::pb::{
    CancelTaskRequest, CreateTaskRequest, GetTaskRequest, ListTasksRequest, ListWorkersRequest,
};
use taskrun_proto::{TaskServiceClient, WorkerServiceClient};

/// TaskRun CLI - Control plane management tool
#[derive(Parser)]
#[command(name = "taskrun")]
#[command(about = "CLI for TaskRun control plane", long_about = None)]
struct Cli {
    /// Control plane address
    #[arg(short, long, default_value = "http://[::1]:50051")]
    addr: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new task
    #[command(name = "create-task")]
    CreateTask {
        /// Agent name to run
        #[arg(short, long)]
        agent: String,

        /// Input JSON for the agent
        #[arg(short, long)]
        input: String,
    },

    /// Get task status
    #[command(name = "get-task")]
    GetTask {
        /// Task ID
        id: String,
    },

    /// List all tasks
    #[command(name = "list-tasks")]
    ListTasks,

    /// List connected workers
    #[command(name = "list-workers")]
    ListWorkers,

    /// Cancel a task
    #[command(name = "cancel-task")]
    CancelTask {
        /// Task ID to cancel
        id: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    let channel = Channel::from_shared(cli.addr)?
        .connect()
        .await?;

    match cli.command {
        Commands::CreateTask { agent, input } => {
            create_task(channel, agent, input).await?;
        }
        Commands::GetTask { id } => {
            get_task(channel, id).await?;
        }
        Commands::ListTasks => {
            list_tasks(channel).await?;
        }
        Commands::ListWorkers => {
            list_workers(channel).await?;
        }
        Commands::CancelTask { id } => {
            cancel_task(channel, id).await?;
        }
    }

    Ok(())
}

async fn create_task(
    channel: Channel,
    agent_name: String,
    input_json: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TaskServiceClient::new(channel);

    let request = CreateTaskRequest {
        agent_name,
        input_json,
        created_by: String::new(),
        labels: std::collections::HashMap::new(),
    };

    let response = client.create_task(request).await?;
    let task = response.into_inner();

    println!("Task created:");
    print_task(&task);

    Ok(())
}

async fn get_task(channel: Channel, id: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TaskServiceClient::new(channel);

    let request = GetTaskRequest { id };

    let response = client.get_task(request).await?;
    let task = response.into_inner();

    print_task(&task);

    Ok(())
}

async fn list_tasks(channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TaskServiceClient::new(channel);

    let request = ListTasksRequest {
        status_filter: 0,  // 0 = no filter
        agent_filter: String::new(),
        limit: 100,
    };

    let response = client.list_tasks(request).await?;
    let resp = response.into_inner();

    println!("Tasks ({}):", resp.tasks.len());
    println!("{:<36}  {:<10}  {:<16}  {}", "ID", "STATUS", "AGENT", "CREATED");
    println!("{}", "-".repeat(80));

    for task in resp.tasks {
        let status = status_name(task.status);
        let created = format_timestamp(task.created_at_ms);
        println!("{:<36}  {:<10}  {:<16}  {}", task.id, status, task.agent_name, created);
    }

    Ok(())
}

async fn list_workers(channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = WorkerServiceClient::new(channel);

    let request = ListWorkersRequest {
        agent_name: None,
        status: None,
    };

    let response = client.list_workers(request).await?;
    let resp = response.into_inner();

    println!("Workers ({}):", resp.workers.len());
    println!("{:<36}  {:<10}  {:<10}  {}", "ID", "STATUS", "RUNS", "AGENTS");
    println!("{}", "-".repeat(80));

    for worker in resp.workers {
        let status = worker_status_name(worker.status);
        let agents: Vec<String> = worker.agents.iter().map(|a| a.name.clone()).collect();
        let agents_str = agents.join(", ");
        let runs = format!("{}/{}", worker.active_runs, worker.max_concurrent_runs);
        println!("{:<36}  {:<10}  {:<10}  {}", worker.worker_id, status, runs, agents_str);
    }

    Ok(())
}

async fn cancel_task(channel: Channel, id: String) -> Result<(), Box<dyn std::error::Error>> {
    let mut client = TaskServiceClient::new(channel);

    let request = CancelTaskRequest { id };

    let response = client.cancel_task(request).await?;
    let task = response.into_inner();

    println!("Task cancelled:");
    print_task(&task);

    Ok(())
}

fn print_task(task: &taskrun_proto::pb::Task) {
    println!("  ID:         {}", task.id);
    println!("  Agent:      {}", task.agent_name);
    println!("  Status:     {}", status_name(task.status));
    println!("  Created:    {}", format_timestamp(task.created_at_ms));

    if !task.runs.is_empty() {
        println!("  Runs:");
        for run in &task.runs {
            let run_status = run_status_name(run.status);
            println!("    - {} ({})", run.run_id, run_status);
            if let Some(backend) = &run.backend_used {
                println!("      Backend: {}/{}", backend.provider, backend.model_name);
            }
        }
    }
}

fn status_name(status: i32) -> &'static str {
    match status {
        0 => "UNSPECIFIED",
        1 => "PENDING",
        2 => "RUNNING",
        3 => "COMPLETED",
        4 => "FAILED",
        5 => "CANCELLED",
        _ => "UNKNOWN",
    }
}

fn run_status_name(status: i32) -> &'static str {
    match status {
        0 => "UNSPECIFIED",
        1 => "PENDING",
        2 => "ASSIGNED",
        3 => "RUNNING",
        4 => "COMPLETED",
        5 => "FAILED",
        6 => "CANCELLED",
        _ => "UNKNOWN",
    }
}

fn worker_status_name(status: i32) -> &'static str {
    match status {
        0 => "UNSPECIFIED",
        1 => "IDLE",
        2 => "BUSY",
        3 => "DRAINING",
        4 => "ERROR",
        _ => "UNKNOWN",
    }
}

fn format_timestamp(ms: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let d = Duration::from_millis(ms as u64);
    let dt = UNIX_EPOCH + d;
    let datetime: chrono::DateTime<chrono::Utc> = dt.into();
    datetime.format("%Y-%m-%d %H:%M:%S").to_string()
}
