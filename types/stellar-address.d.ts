/**
 * Stellar Address Types
 * 
 * TypeScript definitions for Stellar public key and address validation
 */

/**
 * Valid Stellar address format
 * - 56 characters for standard accounts (starting with 'G')
 * - 69 characters for muxed accounts (starting with 'M')
 */
export type StellarAddress = string & { readonly __stellarAddress: unique symbol };
export type MuxedAddress = string & { readonly __muxedAddress: unique symbol };

/**
 * Stellar account types
 */
export type StellarAccount = StandardAccount | MuxedAccount;

export interface StandardAccount {
  type: 'standard';
  address: StellarAddress;
}

export interface MuxedAccount {
  type: 'muxed';
  address: StellarAddress;
  id: string;
  muxedAddress: MuxedAddress;
}

/**
 * Validation error types
 */
export enum ValidationErrorType {
  EmptyAddress = 'EMPTY_ADDRESS',
  InvalidLength = 'INVALID_LENGTH',
  InvalidFormat = 'INVALID_FORMAT',
  InvalidChecksum = 'INVALID_CHECKSUM',
  InvalidEncoding = 'INVALID_ENCODING',
  InvalidMuxedFormat = 'INVALID_MUXED_FORMAT',
  InvalidCharacters = 'INVALID_CHARACTERS',
  UnsupportedVersion = 'UNSUPPORTED_VERSION'
}

export interface ValidationError {
  type: ValidationErrorType;
  message: string;
}

/**
 * Validation result types
 */
export type ValidationResult<T> = 
  | { success: true; data: T }
  | { success: false; error: ValidationError };

/**
 * Stellar Address Validation Utility
 */
export class StellarAddressValidator {
  /**
   * Validate a Stellar address format
   * @param address The address to validate
   * @returns Validation result with parsed account information
   */
  static validate(address: string): ValidationResult<StellarAccount>;

  /**
   * Validate and return a standard Stellar address
   * @param address The address to validate
   * @returns Validation result with standard account
   */
  static validateStandard(address: string): ValidationResult<StandardAccount>;

  /**
   * Validate and return a muxed Stellar address
   * @param address The address to validate
   * @returns Validation result with muxed account
   */
  static validateMuxed(address: string): ValidationResult<MuxedAccount>;

  /**
   * Simple boolean validation
   * @param address The address to validate
   * @returns true if valid, false otherwise
   */
  static isValid(address: string): boolean;

  /**
   * Validate multiple addresses
   * @param addresses Array of addresses to validate
   * @returns Array of validation results
   */
  static validateBatch(addresses: string[]): ValidationResult<StellarAccount>[];

  /**
   * Get error message for validation error type
   * @param errorType The error type
   * @returns Descriptive error message
   */
  static getErrorMessage(errorType: ValidationErrorType): string;
}

/**
 * Convenience functions
 */
export function isValidStellarAddress(address: string): boolean;
export function validateStellarAddress(address: string): ValidationResult<StellarAccount>;
export function validateStandardAddress(address: string): ValidationResult<StandardAccount>;
export function validateMuxedAddress(address: string): ValidationResult<MuxedAccount>;

/**
 * Type assertion functions
 */
export function assertValidStellarAddress(address: string): asserts address is StellarAddress;
export function assertValidMuxedAddress(address: string): asserts address is MuxedAddress;

/**
 * Utility functions
 */
export function isStandardAddress(address: string): boolean;
export function isMuxedAddress(address: string): boolean;
export function extractBaseAddress(muxedAddress: string): StellarAddress | null;