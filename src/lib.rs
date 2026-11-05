#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketplacePlugin {
    pub uuid: String,
    pub name: String,
    pub author: String,
    pub downloads: u64,
}

#[derive(Debug, Default)]
pub struct PluginMarketplaceEngine {
    plugins: Vec<MarketplacePlugin>,
}

impl PluginMarketplaceEngine {
    pub fn new() -> Self { Self { plugins: Vec::new() } }
    pub fn submit_plugin(&mut self, uuid: &str, name: &str, author: &str) {
        self.plugins.push(MarketplacePlugin {
            uuid: uuid.into(),
            name: name.into(),
            author: author.into(),
            downloads: 0,
        });
    }
    pub fn find_by_uuid(&self, uuid: &str) -> Option<&MarketplacePlugin> {
        self.plugins.iter().find(|p| p.uuid == uuid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_plugin_marketplace_engine_aaa() {
        // Arrange
        let mut market = PluginMarketplaceEngine::new();
        // Act
        market.submit_plugin("weather@sigmaos", "Weather Applet", "SigmaOS Team");
        // Assert
        let p = market.find_by_uuid("weather@sigmaos").expect("Plugin should be found");
        assert_eq!(p.name, "Weather Applet");
    }
}
