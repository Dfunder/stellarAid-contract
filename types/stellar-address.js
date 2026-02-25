/**
 * Stellar Address Validation Implementation (JavaScript/TypeScript)
 * 
 * Client-side implementation for validating Stellar addresses
 */

// Base32 alphabet for Stellar addresses
const BASE32_ALPHABET = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';

// Validation error messages
const ERROR_MESSAGES = {
  EMPTY_ADDRESS: "Address cannot be empty",
  INVALID_LENGTH: "Invalid address length - must be 56 characters for standard accounts or 69 for muxed accounts",
  INVALID_FORMAT: "Invalid address format - must start with 'G' for standard accounts or 'M' for muxed accounts",
  INVALID_CHECKSUM: "Address checksum verification failed",
  INVALID_ENCODING: "Invalid base32 encoding in address",
  INVALID_MUXED_FORMAT: "Invalid muxed account format",
  INVALID_CHARACTERS: "Address contains invalid characters",
  UNSUPPORTED_VERSION: "Unsupported Stellar address version"
};

/**
 * Check if a string contains only valid base32 characters
 * @param {string} str - String to validate
 * @returns {boolean} True if valid base32
 */
function isValidBase32(str) {
  for (let i = 0; i < str.length; i++) {
    if (!BASE32_ALPHABET.includes(str[i])) {
      return false;
    }
  }
  return true;
}

/**
 * Validate Stellar address format
 * @param {string} address - Address to validate
 * @returns {Object} Validation result
 */
export function validateStellarAddress(address) {
  // Check if address is empty
  if (!address || address.length === 0) {
    return {
      success: false,
      error: {
        type: 'EMPTY_ADDRESS',
        message: ERROR_MESSAGES.EMPTY_ADDRESS
      }
    };
  }

  // Check length
  const len = address.length;
  if (len !== 56 && len !== 69) {
    return {
      success: false,
      error: {
        type: 'INVALID_LENGTH',
        message: ERROR_MESSAGES.INVALID_LENGTH
      }
    };
  }

  // Check first character
  const firstChar = address[0];
  if (firstChar !== 'G' && firstChar !== 'M') {
    return {
      success: false,
      error: {
        type: 'INVALID_FORMAT',
        message: ERROR_MESSAGES.INVALID_FORMAT
      }
    };
  }

  // Validate characters
  if (!isValidBase32(address)) {
    return {
      success: false,
      error: {
        type: 'INVALID_CHARACTERS',
        message: ERROR_MESSAGES.INVALID_CHARACTERS
      }
    };
  }

  // For muxed accounts, validate the ID portion
  if (len === 69 && firstChar === 'M') {
    const idPortion = address.slice(56);
    if (!isValidBase32(idPortion)) {
      return {
        success: false,
        error: {
          type: 'INVALID_MUXED_FORMAT',
          message: ERROR_MESSAGES.INVALID_MUXED_FORMAT
        }
      };
    }
    
    return {
      success: true,
      data: {
        type: 'muxed',
        address: address.slice(0, 56),
        id: idPortion,
        muxedAddress: address
      }
    };
  }

  // Standard account
  return {
    success: true,
    data: {
      type: 'standard',
      address: address
    }
  };
}

/**
 * Simple boolean validation
 * @param {string} address - Address to validate
 * @returns {boolean} True if valid
 */
export function isValidStellarAddress(address) {
  return validateStellarAddress(address).success;
}

/**
 * Validate standard address only
 * @param {string} address - Address to validate
 * @returns {Object} Validation result
 */
export function validateStandardAddress(address) {
  const result = validateStellarAddress(address);
  if (!result.success) {
    return result;
  }
  
  if (result.data.type !== 'standard') {
    return {
      success: false,
      error: {
        type: 'INVALID_FORMAT',
        message: "Expected standard address (starting with 'G')"
      }
    };
  }
  
  return result;
}

/**
 * Validate muxed address only
 * @param {string} address - Address to validate
 * @returns {Object} Validation result
 */
export function validateMuxedAddress(address) {
  const result = validateStellarAddress(address);
  if (!result.success) {
    return result;
  }
  
  if (result.data.type !== 'muxed') {
    return {
      success: false,
      error: {
        type: 'INVALID_FORMAT',
        message: "Expected muxed address (starting with 'M')"
      }
    };
  }
  
  return result;
}

/**
 * Validate multiple addresses
 * @param {string[]} addresses - Array of addresses to validate
 * @returns {Object[]} Array of validation results
 */
export function validateAddresses(addresses) {
  return addresses.map(address => validateStellarAddress(address));
}

/**
 * Type assertion functions
 */
export function assertValidStellarAddress(address) {
  if (!isValidStellarAddress(address)) {
    throw new Error(`Invalid Stellar address: ${address}`);
  }
}

export function assertValidMuxedAddress(address) {
  const result = validateMuxedAddress(address);
  if (!result.success) {
    throw new Error(`Invalid muxed address: ${result.error.message}`);
  }
}

/**
 * Utility functions
 */
export function isStandardAddress(address) {
  return address && address.length === 56 && address[0] === 'G';
}

export function isMuxedAddress(address) {
  return address && address.length === 69 && address[0] === 'M';
}

export function extractBaseAddress(muxedAddress) {
  if (isMuxedAddress(muxedAddress)) {
    return muxedAddress.slice(0, 56);
  }
  return null;
}

// Export validator class for convenience
export class StellarAddressValidator {
  static validate = validateStellarAddress;
  static validateStandard = validateStandardAddress;
  static validateMuxed = validateMuxedAddress;
  static isValid = isValidStellarAddress;
  static validateBatch = validateAddresses;
  
  static getErrorMessage(errorType) {
    return ERROR_MESSAGES[errorType] || "Unknown error";
  }
}

// Example usage:
/*
const result = StellarAddressValidator.validate("GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37");
if (result.success) {
  console.log("Valid address:", result.data);
} else {
  console.error("Invalid address:", result.error.message);
}

// Simple boolean check
console.log(isValidStellarAddress("GDQP2KPQGKIHYJGXNUIYOMHARUARCA7DJT5FO2FFOOKY3B2WSQHG4W37")); // true
console.log(isValidStellarAddress("invalid")); // false
*/