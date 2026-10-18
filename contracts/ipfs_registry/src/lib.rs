#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String, Vec, Symbol};

/// Issue #580: IPFS document hash registry for invoice verification
/// 
/// Stores content hashes for invoice-supporting documents, allowing investors
/// to verify document integrity against on-chain records.

#[contracttype]
#[derive(Clone, PartialEq)]
pub enum DocumentType {
    InvoicePdf,
    KycCert,
    AuditReport,
    Contract,
}

#[contracttype]
#[derive(Clone)]
pub struct Document {
    pub doc_id: u64,
    pub doc_type: DocumentType,
    pub ipfs_hash: String,
    pub version: u32,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone)]
pub struct DocumentVersion {
    pub version: u32,
    pub ipfs_hash: String,
    pub updated_at: u64,
}

#[contracttype]
pub enum DataKey {
    Issuer(u64), // invoice_id -> issuer address
    Documents(u64), // invoice_id -> Vec<Document>
    DocCounter(u64), // invoice_id -> next doc_id
    VersionHistory(u64, u64), // (invoice_id, doc_id) -> Vec<DocumentVersion>
}

#[contract]
pub struct IpfsRegistryContract;

#[contractimpl]
impl IpfsRegistryContract {
    /// Register a document for an invoice (issuer-only)
    pub fn register_document(
        env: Env,
        issuer: Address,
        invoice_id: u64,
        doc_type: DocumentType,
        ipfs_hash: String,
    ) -> u64 {
        issuer.require_auth();
        
        // Store issuer if first document
        if !env.storage().persistent().has(&DataKey::Issuer(invoice_id)) {
            env.storage()
                .persistent()
                .set(&DataKey::Issuer(invoice_id), &issuer);
        } else {
            // Verify issuer
            let stored_issuer: Address = env
                .storage()
                .persistent()
                .get(&DataKey::Issuer(invoice_id))
                .unwrap();
            if stored_issuer != issuer {
                panic!("Not the issuer");
            }
        }

        let doc_id: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::DocCounter(invoice_id))
            .unwrap_or(0);

        let next_doc_id = doc_id + 1;

        let document = Document {
            doc_id: next_doc_id,
            doc_type: doc_type.clone(),
            ipfs_hash: ipfs_hash.clone(),
            version: 1,
            registered_at: env.ledger().timestamp(),
        };

        // Get existing documents
        let mut docs: Vec<Document> = env
            .storage()
            .persistent()
            .get(&DataKey::Documents(invoice_id))
            .unwrap_or_else(|| Vec::new(&env));

        docs.push_back(document.clone());

        env.storage()
            .persistent()
            .set(&DataKey::Documents(invoice_id), &docs);
        env.storage()
            .persistent()
            .set(&DataKey::DocCounter(invoice_id), &next_doc_id);

        // Initialize version history
        let mut versions = Vec::new(&env);
        versions.push_back(DocumentVersion {
            version: 1,
            ipfs_hash: ipfs_hash.clone(),
            updated_at: env.ledger().timestamp(),
        });
        env.storage()
            .persistent()
            .set(&DataKey::VersionHistory(invoice_id, next_doc_id), &versions);

        // Emit DocumentRegistered event
        env.events().publish(
            (Symbol::new(&env, "DocumentRegistered"), invoice_id),
            (next_doc_id, ipfs_hash),
        );

        next_doc_id
    }

    /// Get all documents for an invoice
    pub fn get_documents(env: Env, invoice_id: u64) -> Vec<Document> {
        env.storage()
            .persistent()
            .get(&DataKey::Documents(invoice_id))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Update a document (issuer-only), appends version history
    pub fn update_document(
        env: Env,
        issuer: Address,
        invoice_id: u64,
        doc_id: u64,
        new_hash: String,
    ) {
        issuer.require_auth();

        // Verify issuer
        let stored_issuer: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Issuer(invoice_id))
            .expect("Invoice not found");
        
        if stored_issuer != issuer {
            panic!("Not the issuer");
        }

        // Get documents
        let mut docs: Vec<Document> = env
            .storage()
            .persistent()
            .get(&DataKey::Documents(invoice_id))
            .expect("Documents not found");

        // Find and update the document
        let mut found = false;
        for i in 0..docs.len() {
            if let Some(doc) = docs.get(i) {
                if doc.doc_id == doc_id {
                    let new_version = doc.version + 1;
                    let updated_doc = Document {
                        doc_id: doc.doc_id,
                        doc_type: doc.doc_type.clone(),
                        ipfs_hash: new_hash.clone(),
                        version: new_version,
                        registered_at: env.ledger().timestamp(),
                    };
                    docs.set(i, updated_doc);
                    found = true;

                    // Append to version history
                    let mut versions: Vec<DocumentVersion> = env
                        .storage()
                        .persistent()
                        .get(&DataKey::VersionHistory(invoice_id, doc_id))
                        .unwrap_or_else(|| Vec::new(&env));

                    versions.push_back(DocumentVersion {
                        version: new_version,
                        ipfs_hash: new_hash.clone(),
                        updated_at: env.ledger().timestamp(),
                    });

                    env.storage()
                        .persistent()
                        .set(&DataKey::VersionHistory(invoice_id, doc_id), &versions);

                    break;
                }
            }
        }

        if !found {
            panic!("Document not found");
        }

        env.storage()
            .persistent()
            .set(&DataKey::Documents(invoice_id), &docs);

        // Emit event
        env.events().publish(
            (Symbol::new(&env, "DocumentUpdated"), invoice_id),
            (doc_id, new_hash),
        );
    }

    /// Get version history for a document
    pub fn get_version_history(env: Env, invoice_id: u64, doc_id: u64) -> Vec<DocumentVersion> {
        env.storage()
            .persistent()
            .get(&DataKey::VersionHistory(invoice_id, doc_id))
            .unwrap_or_else(|| Vec::new(&env))
    }
}
