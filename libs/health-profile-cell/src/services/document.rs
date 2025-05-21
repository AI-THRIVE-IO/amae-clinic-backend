use anyhow::{Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use reqwest::Method;
use serde_json::{json, Value};
use tracing::{debug, error};
use uuid::Uuid;
use std::str::FromStr;

use shared_config::AppConfig;
use shared_database::supabase::SupabaseClient;

use crate::models::Document;

pub struct DocumentService {
    supabase: SupabaseClient,
}

impl DocumentService {
    pub fn new(config: &AppConfig) -> Self {
        Self {
            supabase: SupabaseClient::new(config),
        }
    }
    
pub async fn upload_document(
    &self, 
    patient_id: &str, 
    title: &str,
    base64_file: &str,
    file_type: &str,
    auth_token: &str
) -> Result<Document> {
    debug!("Uploading document for patient: {}", patient_id);
    
    // Validate inputs
    if patient_id.is_empty() {
        return Err(anyhow!("Patient ID cannot be empty"));
    }
    
    if title.is_empty() {
        return Err(anyhow!("Document title cannot be empty"));
    }
    
    // Extract base64 data more robustly
    let base64_data = if base64_file.contains(";base64,") {
        base64_file.split(";base64,").nth(1).unwrap_or(base64_file)
    } else {
        base64_file
    };
    
    // Decode base64 data to bytes with better error handling
    let file_data = match BASE64.decode(base64_data) {
        Ok(data) => data,
        Err(e) => return Err(anyhow!("Failed to decode base64 data: {}", e)),
    };
    
    // Generate a unique filename
    let file_id = Uuid::new_v4().to_string();
    let file_ext = if file_type.contains('/') {
        file_type.split('/').last().unwrap_or("bin")
    } else {
        file_type
    };
    
    let filename = format!("patient-documents/{}/{}.{}", patient_id, file_id, file_ext);
    
    // Upload to Supabase storage
    let path = format!("/storage/v1/object/patient-documents/{}", filename);
    debug!("Uploading to storage path: {}", path);
    
    // Perform upload request
    let upload_result: Value = self.supabase.request(
        Method::POST,
        &path,
        Some(auth_token),
        Some(json!({
            "data": file_data,
            "contentType": file_type
        })),
    ).await?;
    
    debug!("Upload result: {:?}", upload_result);
    
    let storage_path = format!("/storage/v1/object/public/patient-documents/{}", filename);
    // Get public URL
    let public_url = self.supabase.get_public_url(&storage_path);
    debug!("Generated public URL: {}", public_url);
    
    // Create document record in database
    let doc_path = "/rest/v1/documents";
    
    let doc_data = json!({
        "patient_id": patient_id,
        "title": title,
        "file_url": public_url,
        "file_type": file_type,
        "uploaded_at": chrono::Utc::now().to_rfc3339()
    });
    
    // Add Prefer header for the POST request to get back the created record
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        "Prefer",
        reqwest::header::HeaderValue::from_static("return=representation")
    );
    
    let doc_result: Vec<Value> = self.supabase.request_with_headers(
        Method::POST,
        doc_path,
        Some(auth_token),
        Some(doc_data),
        Some(headers),
    ).await?;
    
    if doc_result.is_empty() {
        return Err(anyhow!("Failed to create document record"));
    }
    
    // Parse document with better error handling
    let document: Document = match serde_json::from_value(doc_result[0].clone()) {
        Ok(doc) => doc,
        Err(e) => return Err(anyhow!("Failed to parse document record: {}", e)),
    };
    
    Ok(document)
}
    
    pub async fn get_documents(
        &self, 
        patient_id: &str, 
        auth_token: &str
    ) -> Result<Vec<Document>> {
        debug!("Fetching documents for patient: {}", patient_id);
        
        let path = format!("/rest/v1/documents?patient_id=eq.{}&order=uploaded_at.desc", patient_id);
        
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;
        
        let documents: Vec<Document> = result.into_iter()
            .map(|doc| serde_json::from_value(doc))
            .collect::<std::result::Result<Vec<Document>, _>>()?;
        
        Ok(documents)
    }
    
    pub async fn get_document(
        &self, 
        document_id: &str, 
        auth_token: &str
    ) -> Result<Document> {
        debug!("Fetching document: {}", document_id);
        
        let path = format!("/rest/v1/documents?id=eq.{}", document_id);
        
        let result: Vec<Value> = self.supabase.request(
            Method::GET,
            &path,
            Some(auth_token),
            None,
        ).await?;
        
        if result.is_empty() {
            return Err(anyhow!("Document not found"));
        }
        
        let document: Document = serde_json::from_value(result[0].clone())?;
        Ok(document)
    }
    
    pub async fn delete_document(
        &self, 
        document_id: &str,
        auth_token: &str
    ) -> Result<()> {
        debug!("Deleting document: {}", document_id);
        
        // First get the document to get the file URL
        let doc = self.get_document(document_id, auth_token).await?;
        
        // Extract filename from URL
        if let Some(filename) = doc.file_url.split("patient-documents/").nth(1) {
            // Delete from storage
            let storage_path = format!("/storage/v1/object/patient-documents/{}", filename);
            
            let _: Value = self.supabase.request(
                Method::DELETE,
                &storage_path,
                Some(auth_token),
                None,
            ).await?;
        }
        
        // Delete document record
        let path = format!("/rest/v1/documents?id=eq.{}", document_id);
        
        let _: Value = self.supabase.request(
            Method::DELETE,
            &path,
            Some(auth_token),
            None,
        ).await?;
        
        Ok(())
    }
}