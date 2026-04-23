# Mainnet Configuration Guide

This guide explains how to safely configure StellarAid for mainnet production use.

## ⚠️ Important Security Warnings

1. **Never commit secret keys to version control**
2. **Always use separate keys for testnet and mainnet**
3. **Test thoroughly on testnet before deploying to mainnet**
4. **Keep backup of your keys in a secure location**

## Prerequisites

- Stellar mainnet account with sufficient XLM
- Soroban CLI installed
- Project built and tested on testnet

## Step 1: Generate Mainnet Keypair

```bash
# Generate a new keypair for mainnet
soroban keys generate mainnet_admin --network futurenet

# Or use an existing keypair
# Make sure you have the secret key securely stored
```

## Step 2: Configure Environment Variables

Create or update your `.env` file:

```bash
# Copy example if you haven't already
cp .env.example .env
```

Edit `.env` and set:

```bash
# Switch to mainnet
SOROBAN_NETWORK=mainnet

# Mainnet RPC and Horizon URLs (already configured)
SOROBAN_MAINNET_RPC_URL=https://soroban-rpc.mainnet.stellar.gateway.fm
SOROBAN_MAINNET_PASSPHRASE=Public Global Stellar Network ; September 2015
SOROBAN_MAINNET_HORIZON_URL=https://horizon.stellar.org

# Your mainnet admin keys
SOROBAN_ADMIN_SECRET_KEY=SXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX
SOROBAN_ADMIN_PUBLIC_KEY=GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

# Optional: Issuing account for custom assets
SOROBAN_ISSUING_SECRET_KEY=
SOROBAN_ISSUING_PUBLIC_KEY=
```

## Step 3: Verify Configuration

```bash
# Check configuration
cargo run -p stellaraid-tools -- config

# View vault status (masks secret keys)
cargo run -p stellaraid-tools -- vault

# Toggle to mainnet (validates configuration)
cargo run -p stellaraid-tools -- toggle mainnet
```

## Step 4: Fund Your Account

Ensure your mainnet account has sufficient XLM for:
- Contract deployment (~0.1-0.5 XLM)
- Transaction fees (0.00001 XLM per operation)
- Account reserves (0.5 XLM minimum balance)

## Step 5: Deploy to Mainnet

```bash
# Build the contract
make build-wasm

# Deploy to mainnet
cargo run -p stellaraid-tools -- deploy --network mainnet
```

## Security Checklist

- [ ] Secret keys are in `.env` file
- [ ] `.env` is in `.gitignore`
- [ ] File permissions are restricted (`chmod 600 .env`)
- [ ] Tested on testnet first
- [ ] Account has sufficient XLM balance
- [ ] Backup keys stored securely offline
- [ ] Network is set to `mainnet` in `.env`

## Environment Variables Reference

| Variable | Description | Required |
|----------|-------------|----------|
| `SOROBAN_NETWORK` | Active network (testnet/mainnet) | Yes |
| `SOROBAN_ADMIN_SECRET_KEY` | Admin account secret key | Yes (mainnet) |
| `SOROBAN_ADMIN_PUBLIC_KEY` | Admin account public key | Yes |
| `SOROBAN_ISSUING_SECRET_KEY` | Asset issuing account secret | For custom assets |
| `SOROBAN_ISSUING_PUBLIC_KEY` | Asset issuing account public | For custom assets |

## Troubleshooting

### "SOROBAN_ADMIN_SECRET_KEY is required"
Set your admin secret key in the `.env` file.

### Insufficient balance
Fund your account from an exchange or another wallet.

### RPC connection failed
Check your internet connection and verify the RPC URL is accessible.

## Best Practices

1. **Use Hardware Wallets**: For production, consider using hardware wallets
2. **Multi-signature**: Enable multi-sig for critical operations
3. **Rate Limiting**: Monitor and limit transaction rates
4. **Audit Logs**: Keep detailed logs of all mainnet operations
5. **Regular Backups**: Backup your `.env` file to secure storage

## Need Help?

- Stellar Developers Discord: https://discord.gg/stellardev
- Stellar Developer Documentation: https://developers.stellar.org
- Soroban Documentation: https://soroban.stellar.org
