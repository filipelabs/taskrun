Ok, vamos fazer o que ninguém faz direito: documentar antes de tacar código em `main.rs`.

Segue um documento grande, coeso e usável pra virar `TASKRUN_DESIGN.md` / `ARCHITECTURE.md` no repo.

---

# TaskRun – Visão, Arquitetura e Design Inicial

## 1. Visão geral

**TaskRun** é um **control plane open source** para orquestrar múltiplos agentes de IA executados em workers remotos, usando qualquer modelo, de qualquer provider, com transporte eficiente e seguro entre um servidor central e os workers.

A ideia central:

* Do lado central: você cria **Tasks** (pedidos lógicos).
* Do lado dos workers: essas Tasks viram **Runs** (execuções concretas).
* Tudo via **gRPC streaming bidirecional**, com:

  * baixa latência,
  * suporte a streaming de tokens/output em tempo real,
  * segurança forte (mTLS, CA própria/pinning, possibilidade de assinatura de código).

TaskRun quer ser:

* A “torre de controle” de agentes e modelos heterogêneos.
* Agnóstico a provider (Anthropic, OpenAI, modelos locais, etc.).
* Extensível e simples de integrar.

---

## 2. Objetivos do projeto

### 2.1. Objetivos funcionais

* **Gerenciar múltiplos workers** de forma centralizada.
* **Executar agentes** (flows, logic de orquestração) em workers remotos:

  * cada agente podendo usar diferentes modelos/backends.
* Expor uma API clara para:

  * criar **Tasks**;
  * acompanhar status e resultados;
  * cancelar execuções;
  * inspecionar workers, agentes e modelos disponíveis.

### 2.2. Objetivos não funcionais

* **Eficiência**:

  * uso de gRPC/HTTP2 com streaming bidirecional;
  * overhead mínimo de protocolo;
  * capacidade de lidar com muitos workers simultâneos.
* **Segurança**:

  * canal autenticado e criptografado entre control plane e workers (TLS);
  * autenticação mútua (mTLS);
  * raiz de confiança controlada (CA própria ou public key pinning).
* **Neutralidade de provider**:

  * qualquer modelo, qualquer formato, encaixado num tipo genérico `ModelBackend`.
* **Observabilidade**:

  * visibilidade sobre Tasks, Runs, workers e modelos;
  * suporte a métricas, logs estruturados e auditoria.

---

## 3. Conceitos principais

### 3.1. Task

**Task** é a unidade lógica de trabalho no **control plane**.

* Representa a **intenção** do usuário/sistema:

  * “Roda o agente `support_triage` com esse input.”
* Vive no banco / estado do control plane.
* Campos típicos:

  * `id` – identificador global;
  * `agent_name` – qual agente deve ser usado;
  * `input_json` – payload de entrada em formato neutro;
  * `status` – `PENDING`, `RUNNING`, `COMPLETED`, `FAILED`, `CANCELLED`;
  * `labels` – metadados (tenant, prioridade, tags);
  * `created_at`, `created_by`;
  * `runs` – lista de execuções (`RunSummary`).

Uma Task pode ter **vários Runs** ao longo da vida (retry, fallback, fan-out, etc.).

### 3.2. Run

**Run** é uma **execução concreta** de uma Task em um worker específico.

* Atrelado a:

  * um `task_id`;
  * um `worker_id`;
  * um `agent_name` específico;
  * um `ModelBackend` efetivamente usado.
* Possui:

  * status próprio (`RUNNING`, `COMPLETED`, etc.);
  * timestamps (`started_at`, `finished_at`);
  * informações de backend (modelo, provider);
  * fluxo de output (stream de chunks).

O core do TaskRun é o caminho:

> `Task` (intenção) → `Run` (execução concreta) → output em stream.

### 3.3. Worker

**Worker** é o processo/daemon que roda remotamente e:

* Mantém **conexão persistente** com o control plane;
* Anuncia suas capacidades:

  * quais **Agents** ele suporta;
  * quais **ModelBackends** estão disponíveis;
  * labels (região, hardware, etc.);
* Recebe `RunAssignment` e:

  * executa o agente;
  * streama output (`RunOutputChunk`);
  * atualiza status do Run (`RunStatusUpdate`);
  * envia heartbeats e métricas.

### 3.4. Agent

**Agent** é a lógica de alto nível executada em um worker:

* Pode ser:

  * um flow YAML,
  * um conjunto de ferramentas + prompt,
  * um orquestrador multi-modelo,
  * qualquer coisa que faça sentido no lado do worker.
* O control plane **não precisa saber** a implementação interna:

  * só se importa com `agent_name` e características expostas.

### 3.5. ModelBackend

**ModelBackend** representa um “modelo de IA concreto” por trás do agent:

Exemplo de campos:

* `provider` – `"anthropic"`, `"openai"`, `"ollama"`, `"vllm"`, `"local"`, etc;
* `model_name` – `"claude-3-5-sonnet"`, `"gpt-4.1-mini"`, `"llama-3-8b"`;
* `context_window` – tokens;
* `supports_streaming` – bool;
* `modalities` – lista: `"text"`, `"vision"`, `"audio"`;
* `tools` – nomes lógicos de ferramentas disponíveis;
* `metadata` – mapa livre com config específica do backend.

O control plane usa isso para:

* exibir infos ricas por worker;
* tomar decisões de roteamento (futuro);
* auditoria (`essa Task foi atendida por tal modelo, em tal worker`).

### 3.6. Control plane vs Data plane

* **Control plane**:

  * API para criar/consultar/cancelar Tasks;
  * estado de Tasks, Runs e Workers;
  * scheduler (Task → Run → Worker);
  * controle de segurança (certificados, tokens).
* **Data plane**:

  * Workers;
  * Execução dos agentes e modelos;
  * Streaming de output.

TaskRun implementa diretamente essa separação.

---

## 4. Arquitetura de alto nível

### 4.1. Componentes principais

1. **Control plane (servidor central)**

   * Implementado em Rust.
   * Exposição de:

     * API pública (gRPC/HTTP) para Tasks (`TaskService`).
     * Serviço gRPC streaming bidi com workers (`RunService`).
   * Stores:

     * `TaskStore`, `WorkerStore`, `RunStore` (in-memory no início, depois persistente).

2. **Worker (agent runner)**

   * Binário em Rust.
   * Conecta ao `RunService.Connect` via gRPC.
   * Carrega agents/flows e integra com backends de modelo.
   * Responsável por executar Runs e enviar:

     * chunks de output,
     * updates de status,
     * heartbeats.

3. **CLI / Painel (futuro)**

   * `taskrun-cli` para:

     * criar Tasks;
     * consultar Task/Worker;
     * ajudar no debug.
   * Eventual UI web para visualização de Tasks, Runs e workers.

### 4.2. Comunicação

* **Control plane ↔ Worker**:

  * gRPC streaming bidirecional em HTTP/2.
  * Mensagens de alto nível (`RunClientMessage` / `RunServerMessage`).
  * Conexão persistente (um stream por worker).

* **Cliente ↔ Control plane**:

  * gRPC/HTTP:

    * `TaskService` (Create/Get/List/Cancel).
  * Posteriormente:

    * REST/JSON mirror para facilitar integrações.

---

## 5. Modelo de domínio (nível código)

Resumo dos tipos principais (em Rust / pseudocódigo):

```rust
// IDs
struct TaskId(String);
struct RunId(String);
struct WorkerId(String);

// Status
enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Cancelled,
}

enum RunStatus {
    Pending,
    Assigned,
    Running,
    Completed,
    Failed,
    Cancelled,
}

// Backend de modelo
struct ModelBackend {
    provider: String,
    model_name: String,
    context_window: u32,
    supports_streaming: bool,
    modalities: Vec<String>,
    tools: Vec<String>,
    metadata: HashMap<String, String>,
}

// Agent
struct AgentSpec {
    name: String,
    description: String,
    labels: HashMap<String, String>,
    backends: Vec<ModelBackend>,
}

// Worker
struct WorkerInfo {
    worker_id: WorkerId,
    hostname: String,
    version: String,
    agents: Vec<AgentSpec>,
    labels: HashMap<String, String>,
}

// Task
struct Task {
    id: TaskId,
    agent_name: String,
    input_json: String,
    status: TaskStatus,
    created_by: String,
    created_at: i64,
    labels: HashMap<String, String>,
    runs: Vec<RunSummary>,
}

struct RunSummary {
    run_id: RunId,
    worker_id: WorkerId,
    status: RunStatus,
    started_at: i64,
    finished_at: i64,
    backend_used: Option<ModelBackend>,
}
```

---

## 6. Protocolo gRPC e serviços

### 6.1. TaskService (lado “usuário”)

Responsável por gerenciar **Tasks**.

#### Serviço

```proto
service TaskService {
  rpc CreateTask(CreateTaskRequest) returns (Task);
  rpc GetTask(GetTaskRequest) returns (Task);
  rpc ListTasks(ListTasksRequest) returns (ListTasksResponse);
  rpc CancelTask(CancelTaskRequest) returns (Task);
}
```

#### Mensagens

```proto
message CreateTaskRequest {
  string agent_name = 1;
  string input_json = 2;
  map<string, string> labels = 3;
}

message GetTaskRequest {
  string task_id = 1;
}

message ListTasksRequest {
  uint32 page_size = 1;
  string page_token = 2;
}

message ListTasksResponse {
  repeated Task tasks = 1;
  string next_page_token = 2;
}

message CancelTaskRequest {
  string task_id = 1;
}
```

`Task` e `RunSummary` no proto refletem o modelo de domínio.

### 6.2. RunService (control plane ↔ worker)

Canal streaming bidirecional para:

* registro de worker;
* assignment de Runs;
* streaming de output;
* atualizações de status;
* heartbeats.

#### Serviço

```proto
service RunService {
  rpc Connect(stream RunClientMessage) returns (stream RunServerMessage);
}
```

#### Mensagens principais

```proto
message WorkerHello {
  WorkerInfo info = 1;
}

message WorkerHeartbeat {
  string worker_id = 1;
  string status = 2; // "idle", "busy", "error"
  map<string, string> metrics = 3;
}

message RunAssignment {
  string run_id = 1;
  string task_id = 2;
  string agent_name = 3;
  string input_json = 4;
  map<string, string> labels = 5;
}

message RunOutputChunk {
  string run_id = 1;
  uint64 seq = 2;
  string content = 3;
  bool is_last = 4;
  map<string, string> metadata = 5; // ex: role="assistant"
}

message RunStatusUpdate {
  string run_id = 1;
  RunStatus status = 2;
  string error_message = 3;
  ModelBackend backend_used = 4;
}

message CancelRun {
  string run_id = 1;
}

message RunClientMessage {
  oneof payload {
    WorkerHello hello = 1;
    WorkerHeartbeat heartbeat = 2;
    RunStatusUpdate status_update = 3;
    RunOutputChunk output_chunk = 4;
  }
}

message RunServerMessage {
  oneof payload {
    RunAssignment assign_run = 1;
    CancelRun cancel_run = 2;
  }
}
```

Fluxo típico:

1. Worker conecta e manda `WorkerHello`.
2. Control plane registra/atualiza `WorkerInfo`.
3. Control plane, quando tem Task pendente:

   * cria `Run` para um worker compatível;
   * envia `RunAssignment`.
4. Worker executa:

   * envia `RunStatusUpdate` com `RUNNING`;
   * envia `RunOutputChunk` em stream;
   * finaliza com `RunStatusUpdate` (`COMPLETED` ou `FAILED`).
5. Control plane:

   * atualiza `RunSummary` e `Task.status`.

---

## 7. Segurança e confiança

### 7.1. Ameaças principais

* MITM entre worker e control plane.
* Workers apontando para um “falso control plane”.
* Uso de workers comprometidos.
* Execução de código/flows alterados ou maliciosos.
* Replay de comandos antigos (Runs).

### 7.2. Raiz de confiança

O worker precisa de **um ponto de verdade** inicial:

* Opção A: **CA própria** do servidor:

  * Worker embute o certificado da CA.
  * Só confia em certificados de servidor assinados por essa CA.
* Opção B: **public key pinning**:

  * Worker embute a chave pública esperada do servidor.
  * Na conexão TLS, compara com o certificado apresentado.

Nada de confiar em “todas as CAs do sistema” pra esse canal.

### 7.3. Autenticação mútua (mTLS)

* Conexão worker ↔ control plane usa **mTLS**:

  * Servidor apresenta certificado: worker verifica CA/pin.
  * Worker apresenta certificado cliente: servidor verifica CA interna de workers.
* Cada worker tem:

  * um `client certificate` único;
  * com `subject` ou extensão indicando `worker_id` + atributos (tags, tenant, etc).

### 7.4. Fluxo de enrolment do worker

Primeiro registro de um worker:

1. Operador instala o worker com:

   * binário do `taskrun-worker`;
   * `SERVER_CA_PEM` ou `SERVER_PUB_KEY`;
   * **token de enrolment** (bootstrap), de uso único ou curta validade;
   * configuração básica (nome, labels).

2. Worker:

   * gera par de chaves localmente;
   * abre conexão TLS com servidor, validando certificado do servidor;
   * envia `CSR` + `enrollment_token` para endpoint `/enroll`.

3. Control plane:

   * valida `enrollment_token`;
   * emite certificado de worker:

     * curto prazo de validade (ex: 24h / 7d);
   * retorna cert ao worker.

4. Worker:

   * guarda chave privada + cert;
   * passa a conectar ao `RunService` usando esse cert (mTLS).

### 7.5. Renovação & revogação

* Certs de worker com validade curta:

  * renovação automática via endpoint `/renew`, autenticado com cert atual.
* Control plane pode revogar certificados:

  * revogação por `worker_id` / serial.
* Conexões com cert revogado são recusadas.

### 7.6. Integridade de código/flows (opcional, mas desejável)

Se o control plane distribuir código/flows:

* Bundles de código/flows:

  * são assinados com chave privada de **code signing**.
* Worker:

  * verifica assinatura e hash antes de:

    * aplicar update;
    * rodar novo flow.

### 7.7. Proteção contra replay

* Cada `RunAssignment` inclui:

  * `run_id` único;
  * `task_id`;
  * `issued_at` (timestamp);
  * opcional: `nonce`.
* Worker:

  * mantém cache dos últimos `N` `run_id` e recusa comandos repetidos.
* Control plane:

  * garante unicidade de `run_id`.

---

## 8. Layout de crates e implementação

Projeto Rust estruturado como workspace:

```text
taskrun/
  Cargo.toml              # workspace
  proto/                  # .proto files
  crates/
    taskrun-core/         # domínio (Task, Run, Worker, ModelBackend...)
    taskrun-proto/        # gRPC gerado (tonic/prost) + converters
    taskrun-store/        # traits de storage
    taskrun-store-memory/ # impl em memória dos stores
    taskrun-control-plane/# binário do control plane
    taskrun-worker/       # binário do worker
    taskrun-cli/          # CLI de administração (opcional inicial)
```

### 8.1. `taskrun-core`

* Tipos de domínio:

  * `Task`, `TaskStatus`
  * `RunSummary`, `RunStatus`
  * `WorkerInfo`, `AgentSpec`, `ModelBackend`
  * IDs (`TaskId`, `RunId`, `WorkerId`)
* Não conhece gRPC, DB, rede.

### 8.2. `taskrun-proto`

* Build script roda `tonic-build` em `proto/*.proto`.
* Expõe módulos com `include_proto!`.
* Converters:

  * `From<core::Task> for proto::Task`
  * `TryFrom<proto::Task> for core::Task`
  * etc.

### 8.3. `taskrun-store`

Define traits de storage:

```rust
#[async_trait::async_trait]
pub trait TaskStore: Send + Sync {
    async fn create(&self, task: Task) -> Result<Task>;
    async fn update_status(&self, id: TaskId, status: TaskStatus) -> Result<()>;
    async fn get(&self, id: TaskId) -> Result<Option<Task>>;
    async fn list(&self) -> Result<Vec<Task>>;
}

#[async_trait::async_trait]
pub trait WorkerStore: Send + Sync {
    async fn upsert(&self, worker: WorkerInfo) -> Result<()>;
    async fn get(&self, id: WorkerId) -> Result<Option<WorkerInfo>>;
    async fn list(&self) -> Result<Vec<WorkerInfo>>;
}
```

### 8.4. `taskrun-store-memory`

Implementações em memória, baseadas em `RwLock<HashMap<...>>`.

* Útil para:

  * desenvolvimento local;
  * testes;
  * PoC.

### 8.5. `taskrun-control-plane`

Binário do servidor central.

* Inicialização:

  * carrega config;
  * instancia `InMemoryTaskStore`, `InMemoryWorkerStore` (início);
  * inicia servidor gRPC para:

    * `TaskService`;
    * `RunService`.
* Lógica:

  * gerencia registry de workers conectados (em memória);
  * scheduler simples:

    * escolher worker compatível com `agent_name`;
  * workflows:

    * `CreateTask` → criar `Task` + `Run` interno;
    * enviar `RunAssignment` para worker apropriado;
    * processar `RunOutputChunk` + `RunStatusUpdate`;
    * atualizar Task/Run nos stores.

### 8.6. `taskrun-worker`

Binário do worker.

* Inicialização:

  * lê config local (agents, models, labels);
  * constrói `WorkerInfo`;
  * conecta ao `RunService.Connect` usando mTLS.
* Lógica:

  * manda `WorkerHello`;
  * manda `WorkerHeartbeat` periódicos;
  * recebe `RunAssignment`:

    * executa agente interno (integrações com Anthropic/OpenAI/etc);
    * streama `RunOutputChunk`;
    * manda `RunStatusUpdate`.

### 8.7. `taskrun-cli` (posterior)

* Client simples para:

  * `taskrun task create ...`;
  * `taskrun task get ...`;
  * `taskrun worker list`.

---

## 9. Fluxos principais

### 9.1. Registro de worker

1. Worker sobe.
2. (Se já tem cert) conecta ao control plane via mTLS.
3. Chama `RunService.Connect`.
4. Envia `WorkerHello` com `WorkerInfo`.
5. Control plane:

   * salva/atualiza `WorkerInfo` no `WorkerStore`;
   * associa stream ao `worker_id`.
6. Worker passa a enviar `WorkerHeartbeat` periódicos.

### 9.2. Criação e execução de Task

1. Cliente chama `TaskService.CreateTask` com:

   * `agent_name`
   * `input_json`
   * `labels`.
2. Control plane:

   * cria `Task` com status `PENDING`;
   * escolhe worker compatível (`WorkerInfo.agents` contém `agent_name`);
   * cria `Run` interno para essa Task/worker;
   * envia `RunAssignment` para o worker via `RunService` stream.
3. Worker:

   * recebe `RunAssignment`;
   * instancia execução do agent;
   * manda `RunStatusUpdate` com `RUNNING`;
   * começa a streamar `RunOutputChunk` (sequência de tokens / chunks);
   * ao terminar:

     * manda `RunStatusUpdate` com `COMPLETED` ou `FAILED`, preenchendo `backend_used`.
4. Control plane:

   * atualiza `RunSummary` da Task;
   * atualiza `Task.status` (ex: `COMPLETED` se Run finalizou com sucesso).

### 9.3. Cancelamento de Task/Run

1. Cliente chama `TaskService.CancelTask(task_id)`.
2. Control plane:

   * marca Task como `CANCELLED` (ou “cancela em progresso”);
   * para Runs em andamento:

     * envia `CancelRun` com `run_id` para os workers.
3. Worker:

   * interrompe execução do agente (respeitando cancelamento cooperativo, se suportado);
   * envia `RunStatusUpdate` com `CANCELLED`.

### 9.4. Falha de conexão com worker

* Se a conexão de um worker cai:

  * Control plane marca worker como offline;
  * Runs associados podem ser:

    * marcados como `FAILED`, ou
    * elegíveis para reassignment (política futura).
* Worker, ao voltar:

  * reconecta;
  * envia novo `WorkerHello`;
  * control plane reatach associações e volta a utilizá-lo.

---

## 10. Roadmap inicial (alto nível)

1. **MVP de protocolo & fluxo básico**

   * `taskrun-core`, `taskrun-proto`, `taskrun-store(-memory)`;
   * `TaskService` e `RunService` mínimos;
   * 1 control plane + 1 worker;
   * Task → Run → streaming fake funcionando.

2. **Modelo Task/Run completo**

   * estados definidinhos;
   * `RunSummary` ligado à Task;
   * `WorkerInfo` com `AgentSpec` + `ModelBackend`.

3. **Segurança mínima**

   * TLS obrigatório;
   * CA própria/pinning do servidor;
   * mTLS com cert fixo para dev.

4. **Enrolment & mTLS “de verdade”**

   * fluxo de `/enroll` com CSR + token;
   * emissão de cert individual por worker;
   * renovação e revogação.

5. **Observabilidade e DX**

   * métricas básicas;
   * logs estruturados;
   * CLI ou UI simples para visualizar Tasks/Workers.

6. **Backends de storage reais**

   * Postgres para Tasks / Runs / Workers / tokens;
   * mantenha stores in-memory para testes.

---

## 11. Diretrizes de design

* **Proto-first**:
  Começar sempre definindo mensagens e serviços gRPC, depois implementação.

* **Domínio isolado**:
  `taskrun-core` não sabe nada de network, DB ou runtime.

* **Storage plugável**:
  implementação in-memory primeiro, depois backend real via traits (`TaskStore`, `WorkerStore`, etc).

* **Segurança como requisito, não “add-on”**:
  conexões plaintext e “confio na CA do sistema” não são aceitas no canal control plane ↔ worker.

* **Agnóstico a provider de modelo**:
  tudo que é Anthropic / OpenAI / local fica encapsulado em `ModelBackend` e na lógica do worker.

* **Simples de rodar local**:

  * um binário de control plane;
  * um binário de worker;
  * um comando CLI pra criar Task;
  * tudo funcionando em memória na primeira versão.

---

Pronto. Isso aqui é o “livro de regras” pra TaskRun versão 0.x.
Você consegue pegar esse documento, jogar no repo e começar a quebrar o problema em issues sem se perder no meio do caminho.

