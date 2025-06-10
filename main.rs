// Integration of your original bot with real APIs
// Replace your main.rs with this integrated version

use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, RwLock};
use uuid::Uuid;

// Your original structures (keeping them as-is)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Price {
    pub symbol: String,
    pub price: f64,
    pub timestamp: u64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: f64,
    pub price: Option<f64>,
    pub timestamp: u64,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: f64,
    pub avg_price: f64,
    pub unrealized_pnl: f64,
}

#[derive(Debug, Clone)]
pub struct TradingSignal {
    pub symbol: String,
    pub action: OrderSide,
    pub confidence: f64,
    pub target_price: f64,
    pub quantity: f64,
}

#[derive(Debug, Clone)]
pub struct RiskParams {
    pub max_position_size: f64,
    pub max_loss_per_trade: f64,
    pub max_daily_loss: f64,
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
}

impl Default for RiskParams {
    fn default() -> Self {
        Self {
            max_position_size: 1000.0,
            max_loss_per_trade: 100.0,
            max_daily_loss: 500.0,
            stop_loss_pct: 0.02,
            take_profit_pct: 0.04,
        }
    }
}

// Real API Configuration
#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    pub api_key: String,
    pub secret_key: String,
    pub base_url: String,
    pub testnet: bool,
}

// Binance API Response structures
#[derive(Debug, Deserialize)]
pub struct BinancePrice {
    pub symbol: String,
    pub price: String,
}

#[derive(Debug, Deserialize)]
pub struct BinanceTicker {
    pub symbol: String,
    pub price: String,
    pub volume: String,
}

#[derive(Debug, Deserialize)]
pub struct BinanceOrderBook {
    pub lastUpdateId: u64,
    pub bids: Vec<[String; 2]>,
    pub asks: Vec<[String; 2]>,
}

// Real Binance API implementation
pub struct BinanceAPI {
    client: Client,
    config: ExchangeConfig,
}

impl BinanceAPI {
    pub fn new(config: ExchangeConfig) -> Self {
        Self {
            client: Client::new(),
            config,
        }
    }

    fn generate_signature(&self, query_string: &str) -> String {
        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.secret_key.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(query_string.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    fn get_timestamp(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }

    pub async fn get_price(&self, symbol: &str) -> Result<Price, String> {
        let url = format!("{}/api/v3/ticker/price", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("symbol", symbol)])
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("API error: {}", response.status()));
        }

        let binance_price: BinancePrice = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Get volume separately to avoid error across await
        let volume = match self.get_24hr_volume(symbol).await {
            Ok(v) => v,
            Err(_) => 0.0, // Default volume if fetch fails
        };

        let price = binance_price
            .price
            .parse::<f64>()
            .map_err(|e| format!("Failed to parse price: {}", e))?;

        Ok(Price {
            symbol: binance_price.symbol,
            price,
            timestamp: self.get_timestamp() / 1000,
            volume,
        })
    }

    async fn get_24hr_volume(&self, symbol: &str) -> Result<f64, String> {
        let url = format!("{}/api/v3/ticker/24hr", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("symbol", symbol)])
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let ticker: BinanceTicker = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let volume = ticker
            .volume
            .parse::<f64>()
            .map_err(|e| format!("Failed to parse volume: {}", e))?;
        Ok(volume)
    }

    pub async fn get_orderbook(&self, symbol: &str) -> Result<OrderBook, String> {
        let url = format!("{}/api/v3/depth", self.config.base_url);

        let response = self
            .client
            .get(&url)
            .query(&[("symbol", symbol), ("limit", "10")])
            .send()
            .await
            .map_err(|e| format!("Request failed: {}", e))?;

        let binance_orderbook: BinanceOrderBook = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Parse bids and asks without carrying errors across awaits
        let mut bids = Vec::new();
        for bid in binance_orderbook.bids {
            let price = bid[0]
                .parse::<f64>()
                .map_err(|e| format!("Failed to parse bid price: {}", e))?;
            let quantity = bid[1]
                .parse::<f64>()
                .map_err(|e| format!("Failed to parse bid quantity: {}", e))?;
            bids.push((price, quantity));
        }

        let mut asks = Vec::new();
        for ask in binance_orderbook.asks {
            let price = ask[0]
                .parse::<f64>()
                .map_err(|e| format!("Failed to parse ask price: {}", e))?;
            let quantity = ask[1]
                .parse::<f64>()
                .map_err(|e| format!("Failed to parse ask quantity: {}", e))?;
            asks.push((price, quantity));
        }

        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids,
            asks,
            timestamp: self.get_timestamp() / 1000,
        })
    }

    pub async fn submit_order(&self, order: &Order) -> Result<String, String> {
        if self.config.testnet {
            println!("🧪 TESTNET: Would submit order: {:?}", order);
            tokio::time::sleep(Duration::from_millis(50)).await; // Simulate API delay
            return Ok(format!("testnet_{}", order.id));
        }

        // Real order submission code would go here
        println!("⚠️ LIVE TRADING DISABLED - Set testnet=false and implement real submission");
        Err("Live trading not implemented yet for safety".to_string())
    }
}

// Your original strategy traits and implementations
pub trait TradingStrategy: Send + Sync {
    fn analyze(&self, prices: &[Price], orderbook: &OrderBook) -> Option<TradingSignal>;
    fn name(&self) -> &str;
}

pub struct MomentumStrategy {
    lookback_period: usize,
    momentum_threshold: f64,
}

impl MomentumStrategy {
    pub fn new(lookback_period: usize, momentum_threshold: f64) -> Self {
        Self {
            lookback_period,
            momentum_threshold,
        }
    }
}

impl TradingStrategy for MomentumStrategy {
    fn analyze(&self, prices: &[Price], _orderbook: &OrderBook) -> Option<TradingSignal> {
        if prices.len() < self.lookback_period {
            return None;
        }

        let recent_prices: Vec<f64> = prices
            .iter()
            .rev()
            .take(self.lookback_period)
            .map(|p| p.price)
            .collect();

        if recent_prices.len() < 2 {
            return None;
        }

        let price_change = (recent_prices[0] - recent_prices[recent_prices.len() - 1])
            / recent_prices[recent_prices.len() - 1];

        let volume_avg = prices
            .iter()
            .rev()
            .take(self.lookback_period)
            .map(|p| p.volume)
            .sum::<f64>()
            / self.lookback_period as f64;

        if price_change.abs() > self.momentum_threshold && volume_avg > 1000.0 {
            let action = if price_change > 0.0 {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            };

            return Some(TradingSignal {
                symbol: prices[0].symbol.clone(),
                action,
                confidence: price_change.abs().min(1.0),
                target_price: recent_prices[0],
                quantity: 0.001, // Smaller quantities for testing
            });
        }

        None
    }

    fn name(&self) -> &str {
        "MomentumStrategy"
    }
}

// Risk Manager (keeping your original)
pub struct RiskManager {
    params: RiskParams,
    daily_pnl: Arc<Mutex<f64>>,
    positions: Arc<RwLock<HashMap<String, Position>>>,
}

impl RiskManager {
    pub fn new(params: RiskParams) -> Self {
        Self {
            params,
            daily_pnl: Arc::new(Mutex::new(0.0)),
            positions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn validate_order(&self, order: &Order, current_price: f64) -> bool {
        let daily_pnl = *self.daily_pnl.lock().await;

        if daily_pnl < -self.params.max_daily_loss {
            println!("❌ Order rejected: Daily loss limit exceeded");
            return false;
        }

        let positions = self.positions.read().await;
        if let Some(position) = positions.get(&order.symbol) {
            let new_quantity = match order.side {
                OrderSide::Buy => position.quantity + order.quantity,
                OrderSide::Sell => position.quantity - order.quantity,
            };

            if new_quantity.abs() > self.params.max_position_size {
                println!("❌ Order rejected: Position size limit exceeded");
                return false;
            }
        }

        let potential_loss = order.quantity * current_price * self.params.stop_loss_pct;
        if potential_loss > self.params.max_loss_per_trade {
            println!("❌ Order rejected: Potential loss too high");
            return false;
        }

        true
    }

    pub async fn update_position(&self, symbol: &str, quantity: f64, price: f64) {
        let mut positions = self.positions.write().await;
        let position = positions.entry(symbol.to_string()).or_insert(Position {
            symbol: symbol.to_string(),
            quantity: 0.0,
            avg_price: 0.0,
            unrealized_pnl: 0.0,
        });

        let total_cost = position.quantity * position.avg_price + quantity * price;
        position.quantity += quantity;

        if position.quantity != 0.0 {
            position.avg_price = total_cost / position.quantity;
        }
    }
}

// Updated Market Data Feed using real APIs
pub struct RealMarketDataFeed {
    binance_api: BinanceAPI,
    symbols: Vec<String>,
}

impl RealMarketDataFeed {
    pub fn new(config: ExchangeConfig, symbols: Vec<String>) -> Self {
        Self {
            binance_api: BinanceAPI::new(config),
            symbols,
        }
    }

    pub async fn get_price(&self, symbol: &str) -> Option<Price> {
        match self.binance_api.get_price(symbol).await {
            Ok(price) => {
                println!("📊 Real price for {}: ${:.2}", symbol, price.price);
                Some(price)
            }
            Err(e) => {
                eprintln!("❌ Error fetching price for {}: {}", symbol, e);
                None
            }
        }
    }

    pub async fn get_orderbook(&self, symbol: &str) -> Option<OrderBook> {
        match self.binance_api.get_orderbook(symbol).await {
            Ok(orderbook) => Some(orderbook),
            Err(e) => {
                eprintln!("❌ Error fetching orderbook for {}: {}", symbol, e);
                None
            }
        }
    }
}

// Updated Order Executor using real APIs
pub struct RealOrderExecutor {
    binance_api: BinanceAPI,
    pending_orders: Arc<Mutex<Vec<Order>>>,
}

impl RealOrderExecutor {
    pub fn new(config: ExchangeConfig) -> Self {
        Self {
            binance_api: BinanceAPI::new(config),
            pending_orders: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub async fn submit_order(&self, order: Order) -> Result<String, String> {
        // Add to pending orders first
        {
            let mut pending = self.pending_orders.lock().await;
            pending.push(order.clone());
        }

        // Submit to exchange and handle result immediately
        let result = self.binance_api.submit_order(&order).await;

        match result {
            Ok(order_id) => {
                println!("✅ Order submitted: {}", order_id);
                Ok(order_id)
            }
            Err(error_msg) => {
                println!("❌ Order submission failed: {}", error_msg);

                // Remove from pending orders on failure
                {
                    let mut pending = self.pending_orders.lock().await;
                    pending.retain(|o| o.id != order.id);
                }

                Err(error_msg)
            }
        }
    }

    pub async fn cancel_order(&self, _symbol: &str, order_id: &str) -> Result<(), String> {
        let mut pending = self.pending_orders.lock().await;
        pending.retain(|o| o.id != order_id);
        println!("✅ Order cancelled: {}", order_id);
        Ok(())
    }
}

// Updated Trading Bot with real APIs
pub struct RealTradingBot {
    strategies: Arc<Vec<Box<dyn TradingStrategy>>>,
    risk_manager: Arc<RiskManager>,
    market_feed: Arc<RealMarketDataFeed>,
    order_executor: Arc<RealOrderExecutor>,
    price_history: Arc<RwLock<HashMap<String, Vec<Price>>>>,
    is_running: Arc<Mutex<bool>>,
}

impl RealTradingBot {
    pub fn new(config: ExchangeConfig, symbols: Vec<String>) -> Self {
        let strategies: Vec<Box<dyn TradingStrategy>> = vec![
            Box::new(MomentumStrategy::new(5, 0.00001)), // Ultra-sensitive: 0.001% threshold
        ];

        Self {
            strategies: Arc::new(strategies),
            risk_manager: Arc::new(RiskManager::new(RiskParams::default())),
            market_feed: Arc::new(RealMarketDataFeed::new(config.clone(), symbols.clone())),
            order_executor: Arc::new(RealOrderExecutor::new(config)),
            price_history: Arc::new(RwLock::new(HashMap::new())),
            is_running: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn start(&self, symbols: Vec<String>) {
        *self.is_running.lock().await = true;
        println!("🚀 Starting REAL trading bot for symbols: {:?}", symbols);

        let mut tasks = Vec::new();

        // Start market data collection for each symbol
        for symbol in symbols {
            let symbol_clone = symbol.clone();
            let market_feed = Arc::clone(&self.market_feed);
            let price_history = Arc::clone(&self.price_history);
            let is_running = Arc::clone(&self.is_running);

            let task = tokio::spawn(async move {
                while *is_running.lock().await {
                    if let Some(price) = market_feed.get_price(&symbol_clone).await {
                        let mut history = price_history.write().await;
                        let symbol_history =
                            history.entry(symbol_clone.clone()).or_insert_with(Vec::new);

                        symbol_history.push(price);

                        if symbol_history.len() > 100 {
                            symbol_history.remove(0);
                        }
                    }

                    tokio::time::sleep(Duration::from_secs(5)).await; // Slower for testing
                }
            });

            tasks.push(task);
        }

        // Start trading logic
        let trading_task = self.run_trading_loop().await;
        tasks.push(trading_task);

        futures::future::join_all(tasks).await;
    }

    async fn run_trading_loop(&self) -> tokio::task::JoinHandle<()> {
        let price_history = Arc::clone(&self.price_history);
        let is_running = Arc::clone(&self.is_running);
        let strategies = Arc::clone(&self.strategies);
        let risk_manager = Arc::clone(&self.risk_manager);
        let order_executor = Arc::clone(&self.order_executor);
        let market_feed = Arc::clone(&self.market_feed);

        tokio::spawn(async move {
            while *is_running.lock().await {
                let history = price_history.read().await;

                for (symbol, prices) in history.iter() {
                    println!("📈 Checking {} with {} price points", symbol, prices.len());

                    if prices.len() < 3 {
                        // Reduced from 5 for faster testing
                        continue;
                    }

                    if let Some(orderbook) = market_feed.get_orderbook(symbol).await {
                        for strategy in strategies.iter() {
                            if let Some(signal) = strategy.analyze(prices, &orderbook) {
                                println!("🎯 Signal from {}: {:?}", strategy.name(), signal);

                                let order = Order {
                                    id: Uuid::new_v4().to_string(),
                                    symbol: signal.symbol.clone(),
                                    side: signal.action,
                                    order_type: OrderType::Market,
                                    quantity: signal.quantity,
                                    price: None,
                                    timestamp: SystemTime::now()
                                        .duration_since(UNIX_EPOCH)
                                        .unwrap()
                                        .as_secs(),
                                };

                                if risk_manager
                                    .validate_order(&order, signal.target_price)
                                    .await
                                {
                                    if let Ok(order_id) =
                                        order_executor.submit_order(order.clone()).await
                                    {
                                        println!("✅ Order submitted successfully: {}", order_id);

                                        let quantity = match order.side {
                                            OrderSide::Buy => order.quantity,
                                            OrderSide::Sell => -order.quantity,
                                        };

                                        risk_manager
                                            .update_position(
                                                &order.symbol,
                                                quantity,
                                                signal.target_price,
                                            )
                                            .await;
                                    }
                                } else {
                                    println!("❌ Order rejected by risk manager");
                                }
                            } else {
                                // Debug: Show why no signal was generated
                                if prices.len() >= 3 {
                                    let recent_prices: Vec<f64> =
                                        prices.iter().rev().take(5).map(|p| p.price).collect();
                                    if recent_prices.len() >= 2 {
                                        let price_change = (recent_prices[0]
                                            - recent_prices[recent_prices.len() - 1])
                                            / recent_prices[recent_prices.len() - 1];
                                        println!(
                                            "📊 {} price change: {:.3}% (threshold: 0.001%)",
                                            symbol,
                                            price_change * 100.0
                                        );
                                    }
                                }
                            }
                        }
                    }
                }

                tokio::time::sleep(Duration::from_secs(10)).await; // Conservative frequency
            }
        })
    }

    pub async fn stop(&self) {
        *self.is_running.lock().await = false;
        println!("🛑 Trading bot stopped");
    }
}

// Configuration loader
pub fn load_config() -> ExchangeConfig {
    ExchangeConfig {
        api_key: std::env::var("BINANCE_API_KEY")
            .expect("❌ BINANCE_API_KEY environment variable not set"),
        secret_key: std::env::var("BINANCE_SECRET_KEY")
            .expect("❌ BINANCE_SECRET_KEY environment variable not set"),
        base_url: if std::env::var("USE_TESTNET").unwrap_or_default() == "true" {
            "https://testnet.binance.vision".to_string()
        } else {
            "https://api.binance.com".to_string()
        },
        testnet: std::env::var("USE_TESTNET").unwrap_or_default() == "true",
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let config = load_config();
    println!(
        "🚀 Starting bot in {} mode",
        if config.testnet {
            "🧪 TESTNET"
        } else {
            "🔴 LIVE"
        }
    );

    // Test API connection first
    let api = BinanceAPI::new(config.clone());

    match api.get_price("BTCUSDT").await {
        Ok(price) => println!(
            "✅ API connection successful. BTC price: ${:.2}",
            price.price
        ),
        Err(e) => {
            eprintln!("❌ API connection failed: {}", e);
            return Err(e.into());
        }
    }

    // Define trading symbols
    let symbols = vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()];

    // Create and start the real trading bot
    let bot = RealTradingBot::new(config, symbols.clone());

    println!("🎯 Starting real trading with symbols: {:?}", symbols);

    let bot_task = tokio::spawn(async move {
        bot.start(symbols).await;
    });

    // Run for 120 seconds then stop (for testing)
    tokio::time::sleep(Duration::from_secs(60)).await; // Reduced to 60 seconds for faster testing

    println!("🛑 Shutting down bot...");
    bot_task.abort();

    Ok(())
}
