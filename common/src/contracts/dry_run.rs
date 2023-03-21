use async_trait::async_trait;
use slog::{info, Logger};
use solidity_bindgen::{CallProvider, Context, SendProvider, Web3Context, Web3Provider};
use web3::{
    contract::{
        tokens::{Detokenize, Tokenize},
        Options,
    },
    types::Address,
    Error,
};

pub enum AnyProvider {
    Web3(Web3Provider),
    DryRun(DryRunProvider<Web3Provider>),
}

#[async_trait]
impl CallProvider for AnyProvider {
    async fn call<Out: Detokenize + Unpin + Send, Params: Tokenize + Send>(
        &self,
        name: &'static str,
        params: Params,
    ) -> Result<Out, Error> {
        match self {
            AnyProvider::Web3(web3_provider) => web3_provider.call(name, params).await,
            AnyProvider::DryRun(dry_run) => dry_run.call(name, params).await,
        }
    }
}

#[async_trait]
impl SendProvider for AnyProvider {
    type Out = ();
    async fn send<Params: Tokenize + Send>(
        &self,
        func: &'static str,
        params: Params,
        options: Option<Options>,
        confirmations: Option<usize>,
    ) -> Result<Self::Out, web3::Error> {
        match self {
            AnyProvider::Web3(web3_provider) => web3_provider
                .send(func, params, options, confirmations)
                .await
                .map(|_| ()),
            AnyProvider::DryRun(dry_run) => {
                dry_run.send(func, params, options, confirmations).await
            }
        }
    }
}

#[derive(Clone)]
pub enum AnyContext {
    Web3(Web3Context),
    DryRun(DryRunContext<Web3Context>),
}

impl Context for AnyContext {
    type Provider = AnyProvider;
    fn provider(&self, contract: Address, abi: &[u8]) -> Self::Provider {
        match self {
            AnyContext::Web3(web3) => AnyProvider::Web3(web3.provider(contract, abi)),
            AnyContext::DryRun(dry_run) => AnyProvider::DryRun(dry_run.provider(contract, abi)),
        }
    }
}

pub struct DryRunProvider<BaseProvider> {
    base_provider: BaseProvider,
    logger: Logger,
}

#[derive(Clone)]
pub struct DryRunContext<BaseContext> {
    base_context: BaseContext,
    logger: Logger,
}

impl<BaseContext> DryRunContext<BaseContext>
where
    BaseContext: Context,
{
    pub fn new(base_context: BaseContext, logger: Logger) -> Self {
        Self {
            base_context,
            logger,
        }
    }
}

impl<BaseContext> Context for DryRunContext<BaseContext>
where
    BaseContext: Context,
    BaseContext::Provider: CallProvider,
{
    type Provider = DryRunProvider<BaseContext::Provider>;
    fn provider(&self, contract: Address, abi: &[u8]) -> Self::Provider {
        DryRunProvider::new(
            self.base_context.provider(contract, abi),
            self.logger.clone(),
        )
    }
}

impl<BaseProvider> DryRunProvider<BaseProvider>
where
    BaseProvider: CallProvider,
{
    pub fn new(base_provider: BaseProvider, logger: Logger) -> Self {
        Self {
            base_provider,
            logger,
        }
    }
}

#[async_trait]
impl<BaseProvider: Send + Sync> CallProvider for DryRunProvider<BaseProvider>
where
    BaseProvider: CallProvider,
{
    async fn call<Out: Detokenize + Unpin + Send, Params: Tokenize + Send>(
        &self,
        name: &'static str,
        params: Params,
    ) -> Result<Out, Error> {
        self.base_provider.call(name, params).await
    }
}

#[async_trait]
impl<BaseProvider: Send + Sync> SendProvider for DryRunProvider<BaseProvider> {
    type Out = ();
    async fn send<Params: Tokenize + Send>(
        &self,
        func: &'static str,
        params: Params,
        _options: Option<Options>,
        _confirmations: Option<usize>,
    ) -> Result<Self::Out, web3::Error> {
        let mut tokens = String::new();
        for token in params.into_tokens() {
            let token = format!("{}", token);
            tokens.push_str(&token);
        }
        info!(self.logger, "Send";
            "func" => func,
            "params" => tokens,
        );
        Ok(())
    }
}
